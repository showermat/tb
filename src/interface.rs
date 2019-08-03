use ::format::FmtCmd;
use ::curses;
use ::errors::*;

pub trait Value<'a> {
	fn placeholder(&self) -> FmtCmd;
	fn content(&self) -> FmtCmd;
	fn expandable(&self) -> bool;
	fn children(&self) -> Vec<Box<Value<'a> + 'a>>;
	fn invoke(&self);
}

pub trait Source {
	fn root<'a>(&'a self) -> Box<Value<'a> + 'a>;
}

pub struct Info {
	pub name: &'static str,
	pub desc: &'static str,
}

pub trait Factory {
	fn info(&self) -> Info;
	fn from(&self, &[&str]) -> Option<Result<Box<Source>>>;
	fn colors(&self) -> Vec<curses::Color>;
}
