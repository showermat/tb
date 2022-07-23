use ::interface::*;
use ::interface::fmt::*;
use ::errors::*;

use ::textproto::Value as V;

const HI_STR: usize = 0;
const HI_KWD: usize = 1;
const HI_KEY: usize = 2;
const HI_MUT: usize = 3;
const HI_NUM: usize = 4;

#[derive(Clone, Copy, Debug)]
enum ParentType {
	Root,
	Message,
}

#[derive(Debug)]
pub struct TextprotoValue<'a> {
	key: String,
	value: &'a V,
	parent: ParentType,
}

impl<'a> TextprotoValue<'a> {
	fn fmtstr(s: &str) -> Format {
		super::fmtstr(s, HI_KWD)
	}

	fn fmtkey(&self) -> Format {
		match self.parent {
			ParentType::Root => nosearch(color(HI_MUT, lit("root"))),
			ParentType::Message => noyank(color(HI_KEY, Self::fmtstr(&self.key))),
		}
	}

	fn fmtval(&self) -> Format {
		match self.value {
			V::String(s) => color(HI_STR, Self::fmtstr(s)),
			V::Int(i) => color(HI_NUM, lit(&i.to_string())),
			V::Float(f) => color(HI_NUM, lit(&f.to_string())),
			V::Enum(s) => color(HI_KWD, lit(s)),
			V::Message(items) => nosearch(color(HI_KWD, lit(if items.is_empty() { "{ }" } else { "{...}" }))),
		}
	}
}

impl<'a> Value<'a> for TextprotoValue<'a> {
	fn placeholder(&self) -> Format {
		self.fmtkey()
	}

	fn content(&self) -> Format {
		let sep = match self.value {
			V::Message(_) => " ",
			_ => ": ",
		};
		match self.parent {
			ParentType::Root => self.fmtval(),
			_ => cat(vec![self.fmtkey(), hide(color(HI_MUT, lit(sep))), self.fmtval()]),
		}
	}

	fn expandable(&self) -> bool {
		match *self.value {
			V::Message(_) => true,
			_ => false,
		}
	}

	fn children(&self) -> Vec<Box<dyn Value<'a> + 'a>> {
		match self.value {
			V::Message(items) =>
				items.iter().map(|(k, v)| Box::new(TextprotoValue { key: k.to_string(), value: &v, parent: ParentType::Message }) as Box<dyn Value>).collect(),
			_ => vec![],
		}
	}
}

pub struct TextprotoSource {
	value: V
}

impl TextprotoSource {
	pub fn read<T: std::io::Read>(mut input: T) -> Result<Box<dyn Source>> {
		let mut buf = String::new();
		input.read_to_string(&mut buf).chain_err(|| "failed reading input file to string")?;
		Ok(Box::new(Self { value: textproto::parse(&buf).chain_err(|| "could not parse input as textproto")? }))
	}
}

impl Source for TextprotoSource {
	fn root<'a>(&'a self) -> Box<dyn Value<'a> + 'a> {
		Box::new(TextprotoValue { key: "root".to_string(), value: &self.value, parent: ParentType::Root })
	}
}

pub struct TextprotoFactory { }

impl Factory for TextprotoFactory {
	fn info(&self) -> Info {
		Info { name: "pb", desc: "Browse Protocol Buffer text-format documents" }
	}

	fn from<'a>(&self, args: &[&str]) -> Option<Result<Box<dyn Source>>> {
		match args.get(0) {
			Some(&"-h") | Some(&"--help") => {
				print!(r#"pbb: Browse Protocol Buffer text-format documents interactively

Provide the name of the input file to read as the sole command-line argument, or
provide no arguments to read from standard input.

Part of Tree Browser <https://github.com/showermat/tb>
Copyright (GPLv3) 2020 Matthew Schauer
"#);
				None
			},
			Some(fname) => Some(std::fs::File::open(fname).chain_err(|| "could not open file").and_then(|file| TextprotoSource::read(std::io::BufReader::new(file)))),
			None => {
				let stdin = std::io::stdin();
				let inlock = stdin.lock();
				Some(TextprotoSource::read(inlock))
			},
		}
	}

	fn colors(&self) -> Vec<Color> {
		vec![
			Color { c8: 2, c256: 77 }, // string
			Color { c8: 1, c256: 214 }, // keyword
			Color { c8: 5, c256: 177 }, // key
			Color { c8: 4, c256: 244 }, // muted
			Color { c8: 6, c256: 204 }, // number
		]
	}
}

pub fn get_factory() -> Box<dyn Factory> {
	Box::new(TextprotoFactory { })
}
