// Fichier wrapper pour les bindings rtlsdr générés par bindgen
// Ce fichier inclut les bindings générés au moment du build

#[allow(
	non_camel_case_types,
	non_snake_case,
	non_upper_case_globals,
	dead_code
)]
mod bindings {
	include!(concat!(env!("OUT_DIR"), "/rtlsdr.rs"));
}

pub use bindings::*;
