#[macro_use]
extern crate error_chain;
extern crate enumflags2;
#[macro_use]
extern crate enumflags2_derive;

pub use enumflags2::BitFlags;

pub mod errors {
	error_chain! { }
}

#[derive(EnumFlags, Copy, Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum Render {
	Debug = 0x1,
	Search = 0x2,
	Yank = 0x4,
}

pub enum Format {
	Literal(String),
	Container(Vec<Format>),
	Color(usize, Box<Format>),
	NoBreak(Box<Format>),
	Exclude(BitFlags<Render>, Box<Format>),
}

#[derive(Clone, Copy)]
pub struct Color {
	pub c8: u8,
	pub c256: u8,
}

pub trait Value<'a> {
	fn content(&self) -> Format;
	fn expandable(&self) -> bool;
	fn children(&self) -> Vec<Box<Value<'a> + 'a>>;
	fn placeholder(&self) -> Format { self.content() }
	fn invoke(&self) { }
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
	fn colors(&self) -> Vec<Color> { vec![] }
}

pub mod fmt { // Formatting shortcuts to make tree-building easier
	use super::*;
	pub fn lit(s: &str) -> Format { Format::Literal(s.to_string()) }
	pub fn cat(children: Vec<Format>) -> Format { Format::Container(children) }
	pub fn color(c: usize, child: Format) -> Format { Format::Color(c, Box::new(child)) }
	pub fn nobreak(child: Format) -> Format { Format::NoBreak(Box::new(child)) }
	pub fn exclude(render: BitFlags<Render>, child: Format) -> Format { Format::Exclude(render, Box::new(child)) }
	pub fn nosearch(child: Format) -> Format { Format::Exclude(BitFlags::from(Render::Search), Box::new(child)) }
	pub fn noyank(child: Format) -> Format { Format::Exclude(BitFlags::from(Render::Yank), Box::new(child)) }
	pub fn hide(child: Format) -> Format { Format::Exclude(Render::Search | Render::Yank, Box::new(child)) }
}
