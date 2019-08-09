#[macro_use]
extern crate error_chain;
extern crate tb_interface;
extern crate rand;
extern crate rand_distr;
extern crate serde;
extern crate serde_json;
extern crate reqwest;
extern crate rayon;
extern crate chrono;
extern crate timeago;
extern crate html2text;

use ::tb_interface::*;

mod random;
mod hn;

#[no_mangle]
pub fn get_factories() -> Vec<Box<Factory>> {
	vec![
		Box::new(random::RandFactory { }),
		Box::new(hn::HnFactory { }),
	]
}
