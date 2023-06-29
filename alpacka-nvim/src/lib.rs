use nvim_oxi as oxi;
use oxi::Dictionary;

mod functions;

#[oxi::module]
fn alpacka() -> oxi::Result<Dictionary> {
    Ok(Dictionary::from_iter([("hello", functions::hello())]))
}
