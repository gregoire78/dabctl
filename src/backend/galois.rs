#[allow(dead_code)]
pub struct Galois {
    mm: u16,
    gfpoly: u16,
    code_length: u16,
    d_q: u16,
    alpha_to: Vec<u16>,
    index_of: Vec<u16>,
}

#[allow(dead_code)]
impl Galois {
    pub fn new(symsize: u16, gfpoly: u16) -> Self {
        let code_length = (1u16 << symsize) - 1;
        let d_q = 1u16 << symsize;
        let mut alpha_to = vec![0u16; usize::from(code_length + 1)];
        let mut index_of = vec![0u16; usize::from(code_length + 1)];

        index_of[0] = code_length;
        alpha_to[usize::from(code_length)] = 0;

        let mut sr = 1u16;
        for i in 0..code_length {
            index_of[usize::from(sr)] = i;
            alpha_to[usize::from(i)] = sr;
            sr <<= 1;
            if (sr & (1u16 << symsize)) != 0 {
                sr ^= gfpoly;
            }
            sr &= code_length;
        }

        Self {
            mm: symsize,
            gfpoly,
            code_length,
            d_q,
            alpha_to,
            index_of,
        }
    }

    pub fn code_length(&self) -> u16 {
        self.code_length
    }

    pub fn modnn(&self, mut x: u16) -> u16 {
        while x >= self.code_length {
            x -= self.code_length;
            x = (x >> self.mm) + (x & self.code_length);
        }
        x
    }

    pub fn add_poly(&self, a: u16, b: u16) -> u16 {
        a ^ b
    }

    pub fn poly2power(&self, a: u16) -> u16 {
        self.index_of[usize::from(a)]
    }

    pub fn power2poly(&self, a: u16) -> u16 {
        self.alpha_to[usize::from(a)]
    }

    pub fn add_power(&self, a: u16, b: u16) -> u16 {
        self.index_of[usize::from(self.alpha_to[usize::from(a)] ^ self.alpha_to[usize::from(b)])]
    }

    pub fn multiply_power(&self, a: u16, b: u16) -> u16 {
        self.modnn(a + b)
    }

    pub fn multiply_poly(&self, a: u16, b: u16) -> u16 {
        if a == 0 || b == 0 {
            return 0;
        }
        self.alpha_to[usize::from(
            self.multiply_power(self.index_of[usize::from(a)], self.index_of[usize::from(b)]),
        )]
    }

    pub fn divide_power(&self, a: u16, b: u16) -> u16 {
        self.modnn(self.d_q - 1 + a - b)
    }

    pub fn divide_poly(&self, a: u16, b: u16) -> u16 {
        if a == 0 {
            return 0;
        }
        self.alpha_to[usize::from(
            self.divide_power(self.index_of[usize::from(a)], self.index_of[usize::from(b)]),
        )]
    }

    pub fn inverse_poly(&self, a: u16) -> u16 {
        self.alpha_to[usize::from(self.inverse_power(self.index_of[usize::from(a)]))]
    }

    pub fn inverse_power(&self, a: u16) -> u16 {
        self.d_q - 1 - a
    }

    pub fn pow_poly(&self, a: u16, n: u16) -> u16 {
        self.alpha_to[usize::from(self.pow_power(self.index_of[usize::from(a)], n))]
    }

    pub fn pow_power(&self, a: u16, n: u16) -> u16 {
        if a == 0 {
            0
        } else {
            (a * n) % (self.d_q - 1)
        }
    }
}
