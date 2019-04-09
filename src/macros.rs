#[macro_export]
macro_rules! s {
    ( $e:expr ) => {
        ($e).to_string()
    };
}
