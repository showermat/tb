#[macro_use]
extern crate error_chain;

pub mod errors {
	error_chain! { }
}

pub enum Format {
	Literal(String),
	Container(Vec<Format>),
	Color(usize, Box<Format>),
	NoBreak(Box<Format>),
	Exclude(Box<Format>),
}

impl Format {
	pub fn lit(s: &str) -> Self { Format::Literal(s.to_string()) }
	pub fn cat(children: Vec<Self>) -> Self { Format::Container(children) }
	pub fn color(c: usize, child: Self) -> Self { Format::Color(c, Box::new(child)) }
	pub fn nobreak(child: Self) -> Self { Format::NoBreak(Box::new(child)) }
	pub fn exclude(child: Self) -> Self { Format::Exclude(Box::new(child)) }
}

#[derive(Clone, Copy)]
pub struct Color {
	pub c8: u8,
	pub c256: u8,
}

pub trait Value<'a> {
	fn placeholder(&self) -> Format;
	fn content(&self) -> Format;
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
	fn from(&self, &[&str]) -> Option<errors::Result<Box<Source>>>;
	fn colors(&self) -> Vec<Color>;
}
