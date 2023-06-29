use std::{error::Error, fmt::Display};

use nvim_oxi::Function;

#[derive(Debug)]
pub struct HelloError;

impl Display for HelloError {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        unreachable!()
    }
}

impl Error for HelloError {}

pub fn hello() -> Function<(), ()> {
    Function::from_fn(move |()| {
        nvim_oxi::print!("Hello from alpacka!");

        Ok::<_, HelloError>(())
    })
}
