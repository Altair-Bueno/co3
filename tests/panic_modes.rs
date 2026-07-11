use std::process::Command;

use co3::export;

#[export("C", symbol_prefix = "panic_modes")]
fn out_ptr_panics() -> u8 {
    panic!("out-ptr panic")
}

#[export("C", symbol_prefix = "panic_modes")]
#[ret_val]
fn ret_val_panics() -> u8 {
    panic!("ret-val panic")
}

mod import {
    use co3::ffi;

    ffi! {
        #![extern("C")]
        #![symbol_prefix = "panic_modes"]

        #[symbol_name = "panic_modes__out_ptr_panics"]
        pub fn out_ptr_panics() -> u8;
    }
}

unsafe extern "C" {
    #[symbol_name = "panic_modes__ret_val_panics"]
    fn c_ret_val_panics() -> u8;
}

#[test]
fn out_ptr_panic_returns_status_and_panics_in_rust_wrapper() {
    assert!(std::panic::catch_unwind(import::out_ptr_panics).is_err());
}

#[test]
fn ret_val_panic_aborts() {
    if std::env::var_os("CO3_RET_VAL_ABORT_CHILD").is_some() {
        unsafe {
            c_ret_val_panics();
        }
        unreachable!("ret-val panic did not abort");
    }

    let status = Command::new(std::env::current_exe().expect("current test binary"))
        .arg("--exact")
        .arg("ret_val_panic_aborts")
        .env("CO3_RET_VAL_ABORT_CHILD", "1")
        .status()
        .expect("spawn child test process");

    assert!(!status.success());
}
