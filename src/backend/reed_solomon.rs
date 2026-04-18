use super::galois::Galois;

#[allow(dead_code)]
pub struct ReedSolomon {
    my_galois: Galois,
    symsize: u16,
    code_length: u16,
    generator: Vec<u16>,
    nroots: u16,
    fcr: u16,
    prim: u16,
    iprim: u16,
}

#[allow(dead_code)]
impl ReedSolomon {
    pub fn new() -> Self {
        Self::with_params(8, 0o435, 0, 1, 10)
    }

    pub fn with_params(symsize: u16, gfpoly: u16, fcr: u16, prim: u16, nroots: u16) -> Self {
        let my_galois = Galois::new(symsize, gfpoly);
        let code_length = (1u16 << symsize) - 1;
        let mut iprim = 1u16;
        while !iprim.is_multiple_of(prim) {
            iprim = iprim.wrapping_add(code_length);
        }
        iprim /= prim;

        let mut generator = vec![0u16; usize::from(nroots + 1)];
        generator[0] = 1;

        let mut root = fcr * prim;
        for i in 0..nroots {
            generator[usize::from(i + 1)] = 1;
            let mut j = i;
            while j > 0 {
                let j_usize = usize::from(j);
                if generator[j_usize] != 0 {
                    let p1 =
                        my_galois.multiply_power(my_galois.poly2power(generator[j_usize]), root);
                    generator[j_usize] =
                        my_galois.add_poly(generator[j_usize - 1], my_galois.power2poly(p1));
                } else {
                    generator[j_usize] = generator[j_usize - 1];
                }
                j -= 1;
            }

            generator[0] = my_galois
                .power2poly(my_galois.multiply_power(root, my_galois.poly2power(generator[0])));
            root += 1;
        }

        for value in &mut generator {
            *value = my_galois.poly2power(*value);
        }

        Self {
            my_galois,
            symsize,
            code_length,
            generator,
            nroots,
            fcr,
            prim,
            iprim,
        }
    }

    pub fn enc(&self, r: &[u8], d: &mut [u8], cutlen: i16) {
        let mut rf = vec![0u8; usize::from(self.code_length)];
        let mut bb = vec![0u8; usize::from(self.nroots)];

        let cut = cutlen.max(0) as usize;
        let copy_len = r
            .len()
            .min(usize::from(self.code_length).saturating_sub(cut));
        rf[cut..cut + copy_len].copy_from_slice(&r[..copy_len]);

        self.encode_rs(&rf, &mut bb);

        let data_len = d
            .len()
            .min(usize::from(self.code_length - self.nroots).saturating_sub(cut));
        d[..data_len].copy_from_slice(&rf[cut..cut + data_len]);

        let parity_start = usize::from(self.code_length - self.nroots).saturating_sub(cut);
        for (offset, byte) in bb
            .iter()
            .copied()
            .enumerate()
            .take(usize::from(self.nroots))
        {
            let dst = parity_start + offset;
            if dst < d.len() {
                d[dst] = byte;
            }
        }
    }

    pub fn dec(&self, r: &[u8], d: &mut [u8], cutlen: i16) -> i16 {
        let cut = cutlen.max(0) as usize;
        let mut rf = vec![0u8; usize::from(self.code_length)];

        let copy_len = r
            .len()
            .min(usize::from(self.code_length).saturating_sub(cut));
        rf[cut..cut + copy_len].copy_from_slice(&r[..copy_len]);

        let ret = self.decode_rs(&mut rf);

        let data_len = d
            .len()
            .min(usize::from(self.code_length - self.nroots).saturating_sub(cut));
        d[..data_len].copy_from_slice(&rf[cut..cut + data_len]);

        ret
    }

    fn encode_rs(&self, data: &[u8], bb: &mut [u8]) {
        bb.fill(0);

        for &datum in data
            .iter()
            .take(usize::from(self.code_length - self.nroots))
        {
            let feedback = self
                .my_galois
                .poly2power(self.my_galois.add_poly(u16::from(datum), u16::from(bb[0])));

            if feedback != self.code_length {
                for (j, bb_entry) in bb
                    .iter_mut()
                    .enumerate()
                    .take(usize::from(self.nroots))
                    .skip(1)
                {
                    *bb_entry = self.my_galois.add_poly(
                        u16::from(*bb_entry),
                        self.my_galois.power2poly(self.my_galois.multiply_power(
                            feedback,
                            self.generator[usize::from(self.nroots) - j],
                        )),
                    ) as u8;
                }
            }

            bb.copy_within(1..usize::from(self.nroots), 0);

            bb[usize::from(self.nroots - 1)] = if feedback != self.code_length {
                self.my_galois
                    .power2poly(self.my_galois.multiply_power(feedback, self.generator[0]))
                    as u8
            } else {
                0
            };
        }
    }

    fn decode_rs(&self, data: &mut [u8]) -> i16 {
        let mut syndromes = vec![0u8; usize::from(self.nroots)];
        let mut lambda = vec![0u16; usize::from(self.nroots + 1)];
        let mut root_table = vec![0u16; usize::from(self.nroots)];
        let mut loc_table = vec![0u16; usize::from(self.nroots)];
        let mut omega = vec![0u16; usize::from(self.nroots + 1)];

        if self.compute_syndromes(data, &mut syndromes) {
            return 0;
        }

        let lambda_degree = self.compute_lambda(&syndromes, &mut lambda);
        let mut root_count =
            self.compute_errors(&lambda, lambda_degree, &mut root_table, &mut loc_table);
        if root_count < 0 {
            return -1;
        }

        let omega_degree = self.compute_omega(&syndromes, &lambda, lambda_degree, &mut omega);

        let mut j = root_count - 1;
        loop {
            let j_usize = j as usize;
            let mut num1 = 0u16;
            let mut i = omega_degree as i32;
            while i >= 0 {
                let idx = i as usize;
                if omega[idx] != self.code_length {
                    let tmp = self.my_galois.multiply_power(
                        omega[idx],
                        self.my_galois.pow_power(i as u16, root_table[j_usize]),
                    );
                    num1 = self
                        .my_galois
                        .add_poly(num1, self.my_galois.power2poly(tmp));
                }
                i -= 1;
            }

            let tmp = self.my_galois.multiply_power(
                self.my_galois.pow_power(
                    root_table[j_usize],
                    self.my_galois.divide_power(self.fcr, 1),
                ),
                self.code_length,
            );
            let num2 = self.my_galois.power2poly(tmp);

            let mut den = 0u16;
            let mut i = (u16::min(lambda_degree, self.nroots - 1) & !1) as i32;
            while i >= 0 {
                let idx = i as usize;
                if lambda[idx + 1] != self.code_length {
                    let tmp = self.my_galois.multiply_power(
                        lambda[idx + 1],
                        self.my_galois.pow_power(i as u16, root_table[j_usize]),
                    );
                    den = self.my_galois.add_poly(den, self.my_galois.power2poly(tmp));
                }
                i -= 2;
            }

            if den == 0 {
                return -1;
            }

            if num1 != 0 {
                if loc_table[j_usize] >= self.code_length - self.nroots {
                    root_count -= 1;
                } else {
                    let tmp1 = self.code_length - self.my_galois.poly2power(den);
                    let mut tmp2 = self.my_galois.multiply_power(
                        self.my_galois.poly2power(num1),
                        self.my_galois.poly2power(num2),
                    );
                    tmp2 = self.my_galois.multiply_power(tmp2, tmp1);
                    let corr = self.my_galois.power2poly(tmp2) as u8;
                    let loc = usize::from(loc_table[j_usize]);
                    data[loc] = self
                        .my_galois
                        .add_poly(u16::from(data[loc]), u16::from(corr))
                        as u8;
                }
            }

            if j == 0 {
                break;
            }
            j -= 1;
        }

        root_count
    }

    fn get_syndrome(&self, data: &[u8], root: u16) -> u8 {
        let mut syn = data[0];

        for byte in data.iter().take(usize::from(self.code_length)).skip(1) {
            if syn == 0 {
                syn = *byte;
            } else {
                let uu1 = self
                    .my_galois
                    .pow_power(self.my_galois.multiply_power(self.fcr, root), self.prim);
                syn = self.my_galois.add_poly(
                    u16::from(*byte),
                    self.my_galois.power2poly(
                        self.my_galois
                            .multiply_power(self.my_galois.poly2power(u16::from(syn)), uu1),
                    ),
                ) as u8;
            }
        }

        syn
    }

    fn compute_syndromes(&self, data: &[u8], syndromes: &mut [u8]) -> bool {
        let mut syn_error = 0u16;

        for (i, syndrome) in syndromes
            .iter_mut()
            .enumerate()
            .take(usize::from(self.nroots))
        {
            *syndrome = self.get_syndrome(data, i as u16);
            syn_error |= u16::from(*syndrome);
        }

        syn_error == 0
    }

    fn compute_lambda(&self, syndromes: &[u8], lambda: &mut [u16]) -> u16 {
        let mut k = 1usize;
        let mut l = 0usize;
        let mut corrector = vec![0u16; usize::from(self.nroots + 1)];
        let mut deg_lambda = 0u16;

        for value in lambda.iter_mut() {
            *value = 0;
        }

        let mut error = u16::from(syndromes[0]);
        lambda[0] = 1;
        if self.nroots > 1 {
            corrector[1] = 1;
        }

        while k < usize::from(self.nroots) {
            let old_lambda = lambda.to_vec();

            for i in 0..usize::from(self.nroots) {
                lambda[i] = self
                    .my_galois
                    .add_poly(lambda[i], self.my_galois.multiply_poly(error, corrector[i]));
            }

            if (2 * l < k) && (error != 0) {
                l = k - l;
                for i in 0..usize::from(self.nroots) {
                    corrector[i] = self.my_galois.divide_poly(old_lambda[i], error);
                }
            }

            for i in (1..usize::from(self.nroots)).rev() {
                corrector[i] = corrector[i - 1];
            }
            corrector[0] = 0;

            error = u16::from(syndromes[k]);
            for i in 1..=k {
                error = self.my_galois.add_poly(
                    error,
                    self.my_galois
                        .multiply_poly(u16::from(syndromes[k - i]), lambda[i]),
                );
            }
            k += 1;
        }

        for i in 0..usize::from(self.nroots) {
            lambda[i] = self
                .my_galois
                .add_poly(lambda[i], self.my_galois.multiply_poly(error, corrector[i]));
        }

        for (i, lambda_entry) in lambda.iter_mut().enumerate().take(usize::from(self.nroots)) {
            if *lambda_entry != 0 {
                deg_lambda = i as u16;
            }
            *lambda_entry = self.my_galois.poly2power(*lambda_entry);
        }

        deg_lambda
    }

    fn compute_errors(
        &self,
        lambda: &[u16],
        deg_lambda: u16,
        root_table: &mut [u16],
        loc_table: &mut [u16],
    ) -> i16 {
        let mut root_count = 0usize;
        let mut work_register = lambda.to_vec();
        let mut k = self.iprim - 1;

        for i in 1..=self.code_length {
            let mut result = 1u16;
            let mut j = deg_lambda as i32;
            while j > 0 {
                let idx = j as usize;
                if work_register[idx] != self.code_length {
                    work_register[idx] =
                        self.my_galois.multiply_power(work_register[idx], j as u16);
                    result = self
                        .my_galois
                        .add_poly(result, self.my_galois.power2poly(work_register[idx]));
                }
                j -= 1;
            }

            if result == 0 {
                root_table[root_count] = i;
                loc_table[root_count] = k;
                root_count += 1;
            }
            k = k.wrapping_add(self.iprim);
        }

        if root_count != usize::from(deg_lambda) {
            -1
        } else {
            root_count as i16
        }
    }

    fn compute_omega(
        &self,
        syndromes: &[u8],
        lambda: &[u16],
        deg_lambda: u16,
        omega: &mut [u16],
    ) -> u16 {
        let mut deg_omega = 0u16;

        for i in 0..usize::from(self.nroots) {
            let mut tmp = 0u16;
            let mut j = if usize::from(deg_lambda) < i {
                deg_lambda as i32
            } else {
                i as i32
            };

            while j >= 0 {
                let j_usize = j as usize;
                let syn = u16::from(syndromes[i - j_usize]);
                if self.my_galois.poly2power(syn) != self.code_length
                    && lambda[j_usize] != self.code_length
                {
                    let res = self.my_galois.power2poly(
                        self.my_galois
                            .multiply_power(self.my_galois.poly2power(syn), lambda[j_usize]),
                    );
                    tmp = self.my_galois.add_poly(tmp, res);
                }
                j -= 1;
            }

            if tmp != 0 {
                deg_omega = i as u16;
            }
            omega[i] = self.my_galois.poly2power(tmp);
        }

        omega[usize::from(self.nroots)] = self.code_length;
        deg_omega
    }
}

impl Default for ReedSolomon {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::ReedSolomon;

    #[test]
    fn corrects_one_shortened_symbol_error() {
        let rs = ReedSolomon::new();
        let mut message = [0u8; 110];
        for (idx, byte) in message.iter_mut().enumerate() {
            *byte = ((idx * 17 + 3) & 0xff) as u8;
        }

        let mut encoded = [0u8; 120];
        rs.enc(&message, &mut encoded, 135);
        encoded[37] ^= 0x55;

        let mut decoded = [0u8; 110];
        let corrected = rs.dec(&encoded, &mut decoded, 135);

        assert!(corrected >= 0);
        assert_eq!(decoded, message);
    }
}
