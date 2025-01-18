macro_rules! fail {
    ($code: expr, $e:expr) => {
        let current_exe = std::env::current_exe().unwrap();
        eprintln!("{}: {}", current_exe.file_name().unwrap().to_string_lossy(), $e);
        // std::process::exit($code);
        return Err($code);
    };
    ($code:expr, $($tts:tt)*) => {
        let current_exe = std::env::current_exe().unwrap();
        eprintln!("{}: {}", current_exe.file_name().unwrap().to_string_lossy(), format!($($tts)*));
        // std::process::exit($code);
        return Err($code);
    };
}

macro_rules! unwrap_or_fail {
    ($code: expr, $value: expr) => {
        match $value {
            Ok(v) => v,
            Err(e) => {
                fail!($code, e.to_string());
            }
        }
    }
}


pub type VisResult<T> = std::result::Result<T, u8>;

pub(crate) use fail;
pub(crate) use unwrap_or_fail;

