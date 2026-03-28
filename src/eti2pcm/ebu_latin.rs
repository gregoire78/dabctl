/// Conversion EBU Latin vers String UTF-8 (logique DABlin, tout-en-un)
pub fn ebu_latin_to_utf8(ch: u8) -> String {
    // LUT pour 0x00-0x1F (alignée sur DABlin)
    const EBU_0X00_0X1F: [&str; 0x20] = [
        "", "\u{0118}", "\u{012E}", "\u{0172}", "\u{0102}", "\u{0116}", "\u{010E}", "\u{0218}",
        "\u{021A}", "\u{010A}", "", "", "\u{0120}", "\u{0139}", "\u{017B}", "\u{0143}",
        "\u{0105}", "\u{0119}", "\u{012F}", "\u{0173}", "\u{0103}", "\u{0117}", "\u{010F}", "\u{0219}",
        "\u{021B}", "\u{010B}", "\u{0147}", "\u{011A}", "\u{0121}", "\u{013A}", "\u{017C}", ""
    ];
    // LUT pour 0x7B-0xFF (alignée sur DABlin)
    const EBU_0X7B_0XFF: [&str; 133] = [
        "\u{00AB}", "\u{016F}", "\u{00BB}", "\u{013D}", "\u{0126}",
        "\u{00E1}", "\u{00E0}", "\u{00E9}", "\u{00E8}", "\u{00ED}", "\u{00EC}", "\u{00F3}", "\u{00F2}", "\u{00FA}", "\u{00F9}", "\u{00D1}", "\u{00C7}", "\u{015E}", "\u{00DF}", "\u{00A1}", "\u{0178}",
        "\u{00E2}", "\u{00E4}", "\u{00EA}", "\u{00EB}", "\u{00EE}", "\u{00EF}", "\u{00F4}", "\u{00F6}", "\u{00FB}", "\u{00FC}", "\u{00F1}", "\u{00E7}", "\u{015F}", "\u{011F}", "\u{0131}", "\u{00FF}",
        "\u{0136}", "\u{0145}", "\u{00A9}", "\u{0122}", "\u{011E}", "\u{011B}", "\u{0148}", "\u{0151}", "\u{0150}", "\u{20AC}", "\u{00A3}", "\u{0024}", "\u{0100}", "\u{0112}", "\u{012A}", "\u{016A}",
        "\u{0137}", "\u{0146}", "\u{013B}", "\u{0123}", "\u{013C}", "\u{0130}", "\u{0144}", "\u{0171}", "\u{0170}", "\u{00BF}", "\u{013E}", "\u{00B0}", "\u{0101}", "\u{0113}", "\u{012B}", "\u{016B}",
        "\u{00C1}", "\u{00C0}", "\u{00C9}", "\u{00C8}", "\u{00CD}", "\u{00CC}", "\u{00D3}", "\u{00D2}", "\u{00DA}", "\u{00D9}", "\u{0158}", "\u{010C}", "\u{0160}", "\u{017D}", "\u{00D0}", "\u{013F}",
        "\u{00C2}", "\u{00C4}", "\u{00CA}", "\u{00CB}", "\u{00CE}", "\u{00CF}", "\u{00D4}", "\u{00D6}", "\u{00DB}", "\u{00DC}", "\u{0159}", "\u{010D}", "\u{0161}", "\u{017E}", "\u{0111}", "\u{0140}",
        "\u{00C3}", "\u{00C5}", "\u{00C6}", "\u{0152}", "\u{0177}", "\u{00DD}", "\u{00D5}", "\u{00D8}", "\u{00DE}", "\u{014A}", "\u{0154}", "\u{0106}", "\u{015A}", "\u{0179}", "\u{0164}", "\u{00F0}",
        "\u{00E3}", "\u{00E5}", "\u{00E6}", "\u{0153}", "\u{0175}", "\u{00FD}", "\u{00F5}", "\u{00F8}", "\u{00FE}", "\u{014B}", "\u{0155}", "\u{0107}", "\u{015B}", "\u{017A}", "\u{0165}", "\u{0127}"
    ];
    if ch <= 0x1F {
        return EBU_0X00_0X1F[ch as usize].to_string();
    }
    if ch >= 0x7B {
        return EBU_0X7B_0XFF[(ch - 0x7B) as usize].to_string();
    }
    match ch {
        0x24 => return "\u{0142}".to_string(), // ł
        0x5C => return "\u{016E}".to_string(), // Ů
        0x5E => return "\u{0141}".to_string(), // Ł
        0x60 => return "\u{0104}".to_string(), // Ą
        _ => {}
    }
    // ASCII direct sinon
    (ch as char).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ebu_latin_to_utf8() {
        assert_eq!(ebu_latin_to_utf8(b'A'), "A");
        assert_eq!(ebu_latin_to_utf8(0x00), "");
        assert_eq!(ebu_latin_to_utf8(0x01), "\u{0118}");
        assert_eq!(ebu_latin_to_utf8(0x0A), "");
        assert_eq!(ebu_latin_to_utf8(0x7B), "\u{00AB}");
        assert_eq!(ebu_latin_to_utf8(0xF3), "\u{0153}");
        assert_eq!(ebu_latin_to_utf8(0x24), "\u{0142}");
        assert_eq!(ebu_latin_to_utf8(0x5C), "\u{016E}");
        assert_eq!(ebu_latin_to_utf8(0x5E), "\u{0141}");
        assert_eq!(ebu_latin_to_utf8(0x60), "\u{0104}");
    }
}
