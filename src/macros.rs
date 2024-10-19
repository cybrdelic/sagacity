// src/macros.rs

#[macro_export]
macro_rules! debug_print {
    ($($arg:tt)*) => {
        if cfg!(debug_assertions) {
            println!($($arg)*);
        }
    };
}
