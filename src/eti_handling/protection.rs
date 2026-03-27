// ==============================================================================
// eti_handling/protection.rs - Protection schemes (UEP, EEP)
// ==============================================================================

use anyhow::Result;

/// Schéma de protection UEP (Unequal Error Protection)
#[derive(Debug, Clone)]
pub struct UepProtection {
    /// Niveau de protection (1-4, 4 = plus protégé)
    pub level: u8,
    /// Longueur du subchannel en CUs
    pub length: u16,
}

impl UepProtection {
    /// Créer une protection UEP
    pub fn new(level: u8, length: u16) -> Result<Self> {
        if !(1..=4).contains(&level) {
            return Err(anyhow::anyhow!("UEP level must be 1-4, got {}", level));
        }

        Ok(Self { level, length })
    }

    /// Obtenir le nombre de bits d'information
    pub fn info_bits(&self) -> u32 {
        let cu_bits = 64; // 1 CU = 64 bits
        (self.length as u32) * cu_bits
    }

    /// Obtenir le codage de protection (taux de redondance)
    pub fn code_rate(&self) -> f32 {
        match self.level {
            1 => 1.0 / 3.0,   // 1/3
            2 => 2.0 / 5.0,   // 2/5
            3 => 1.0 / 2.0,   // 1/2
            4 => 3.0 / 4.0,   // 3/4
            _ => 0.0,
        }
    }
}

/// Schéma de protection EEP (Equal Error Protection)
#[derive(Debug, Clone)]
pub struct EepProtection {
    /// Niveau de protection (A, B)
    pub level: char, // 'A' ou 'B'
    /// Longueur du subchannel
    pub length: u16,
}

impl EepProtection {
    /// Créer une protection EEP
    pub fn new(level: char, length: u16) -> Result<Self> {
        if level != 'A' && level != 'B' {
            return Err(anyhow::anyhow!("EEP level must be A or B, got {}", level));
        }

        Ok(Self { level, length })
    }

    /// Obtenir le codage de protection
    pub fn code_rate(&self) -> f32 {
        match self.level {
            'A' => 1.0 / 2.0,   // 1/2
            'B' => 3.0 / 4.0,   // 3/4
            _ => 0.0,
        }
    }
}

/// Type de protection unifié
#[derive(Debug, Clone)]
pub enum ProtectionScheme {
    Uep(UepProtection),
    Eep(EepProtection),
}

impl ProtectionScheme {
    /// Obtenir le taux de codage
    pub fn code_rate(&self) -> f32 {
        match self {
            Self::Uep(u) => u.code_rate(),
            Self::Eep(e) => e.code_rate(),
        }
    }

    /// Obtenir la longueur en CUs
    pub fn length(&self) -> u16 {
        match self {
            Self::Uep(u) => u.length,
            Self::Eep(e) => e.length,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uep_protection() {
        let uep = UepProtection::new(1, 100).unwrap();
        assert_eq!(uep.level, 1);
        assert_eq!(uep.code_rate(), 1.0 / 3.0);
    }

    #[test]
    fn test_uep_invalid_level() {
        assert!(UepProtection::new(5, 100).is_err());
    }

    #[test]
    fn test_eep_protection() {
        let eep = EepProtection::new('A', 100).unwrap();
        assert_eq!(eep.level, 'A');
        assert_eq!(eep.code_rate(), 1.0 / 2.0);
    }

    #[test]
    fn test_protection_scheme_uep() {
        let proto = ProtectionScheme::Uep(UepProtection::new(2, 50).unwrap());
        assert_eq!(proto.code_rate(), 2.0 / 5.0);
        assert_eq!(proto.length(), 50);
    }
}
