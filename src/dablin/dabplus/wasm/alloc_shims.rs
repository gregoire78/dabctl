use std::alloc::{alloc, alloc_zeroed, dealloc, Layout};
use std::ffi::c_void;
use std::ptr;

const MAX_ALIGN: usize = 8;
const HEADER_SIZE: usize = std::mem::size_of::<AllocHeader>();

#[repr(C)]
struct AllocHeader {
    size: usize,
    align: usize,
}

fn header_layout() -> Layout {
    Layout::new::<AllocHeader>()
}

fn alloc_with_header(size: usize, zeroed: bool) -> *mut c_void {
    if size == 0 {
        return ptr::null_mut();
    }

    let Ok(base_layout) = Layout::from_size_align(size, MAX_ALIGN) else {
        return ptr::null_mut();
    };
    let Ok((layout, _)) = header_layout().extend(base_layout) else {
        return ptr::null_mut();
    };
    let layout = layout.pad_to_align();

    let raw = unsafe {
        if zeroed {
            alloc_zeroed(layout)
        } else {
            alloc(layout)
        }
    };
    if raw.is_null() {
        return ptr::null_mut();
    }

    let header = raw as *mut AllocHeader;
    unsafe {
        (*header).size = size;
        (*header).align = MAX_ALIGN;
    }

    unsafe { raw.add(HEADER_SIZE) as *mut c_void }
}

fn free_with_header(ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }

    let raw = unsafe { (ptr as *mut u8).sub(HEADER_SIZE) };
    let header = raw as *mut AllocHeader;
    let size = unsafe { (*header).size };
    let align = unsafe { (*header).align };

    if size == 0 || align == 0 || !align.is_power_of_two() {
        return;
    }

    let Ok(base_layout) = Layout::from_size_align(size, align) else {
        return;
    };
    let Ok((full_layout, _)) = header_layout().extend(base_layout) else {
        return;
    };
    let full_layout = full_layout.pad_to_align();

    unsafe { dealloc(raw, full_layout) };
}

// libfec C objects (compiled for wasm32-unknown-unknown) reference malloc/calloc/free.
// On this target there is no libc, so we provide minimal symbols to satisfy linking.
// We store a small header before each allocation so free() can deallocate correctly.
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub unsafe extern "C" fn malloc(size: usize) -> *mut c_void {
    alloc_with_header(size, false)
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub unsafe extern "C" fn calloc(nmemb: usize, size: usize) -> *mut c_void {
    let Some(total) = nmemb.checked_mul(size) else {
        return ptr::null_mut();
    };
    alloc_with_header(total, true)
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub unsafe extern "C" fn free(ptr: *mut c_void) {
    free_with_header(ptr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malloc_zero_returns_null() {
        assert!(alloc_with_header(0, false).is_null());
    }

    #[test]
    fn calloc_overflow_returns_null() {
        let p = (usize::MAX, 2usize);
        let total = p.0.checked_mul(p.1);
        assert!(total.is_none());
    }

    #[test]
    fn alloc_and_free_roundtrip() {
        let p = alloc_with_header(64, false);
        assert!(!p.is_null());
        free_with_header(p);
    }

    #[test]
    fn calloc_is_zeroed() {
        let p = alloc_with_header(32, true);
        assert!(!p.is_null());

        let bytes = unsafe { std::slice::from_raw_parts(p as *const u8, 32) };
        assert!(bytes.iter().all(|b| *b == 0));

        free_with_header(p);
    }
}
