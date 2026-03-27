use eti_rtlsdr_rust::support::dab_params::DabParams;
use eti_rtlsdr_rust::eti_handling::fic_handler::FicHandler;

#[test]
fn fic_handler_creation_mode1() {
    let params = DabParams::new(1);
    let fh = FicHandler::new(&params);
    // Initial quality should be 0
    assert_eq!(fh.get_fic_quality(), 0);
}

#[test]
fn fic_handler_creation_mode2() {
    let params = DabParams::new(2);
    let _fh = FicHandler::new(&params);
}

#[test]
fn fic_handler_channel_data_initially_unused() {
    let params = DabParams::new(1);
    let fh = FicHandler::new(&params);
    for i in 0..64 {
        let cd = fh.get_channel_info(i);
        assert!(!cd.in_use, "Channel {} should not be in use initially", i);
    }
}

#[test]
fn fic_handler_cif_count_initial() {
    let params = DabParams::new(1);
    let fh = FicHandler::new(&params);
    let (hi, lo) = fh.get_cif_count();
    // Initial value is -1 (not yet received from FIG0/0)
    assert_eq!(hi, -1);
    assert_eq!(lo, -1);
}

#[test]
fn fic_handler_process_zero_block() {
    let params = DabParams::new(1);
    let mut fh = FicHandler::new(&params);
    let bits_per_block = 2 * params.k as usize;
    let data = vec![0i16; bits_per_block * 3]; // 3 FIC blocks for mode 1
    let mut out = vec![0u8; 768]; // 4 FIBs * 32 bytes * 6 = enough space
    let mut valid = vec![false; 4]; // 3*3072/2304 = 4 FIC segments
    fh.process_fic_block(&data, &mut out, &mut valid);
    // With all-zero input, FIC quality should reflect processing happened
    // Even if CRC fails, the function should not panic
}
