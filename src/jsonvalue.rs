use ::errors::*;
use ::disptree::{DispSource, DispValue};
use ::format::FmtCmd;
use ::curses::Color;
use ::serde_json::{from_reader, Value};

const HI_STR: usize = 0;
const HI_KWD: usize = 1;
const HI_KEY: usize = 2;
const HI_MUT: usize = 3;

#[derive(Clone, Copy, Debug)]
enum ParentType {
	Root,
	Object,
	Array,
}

#[derive(Debug)]
pub struct JsonValue<'a> {
	key: String,
	value: &'a Value,
	parent: ParentType,
}

impl<'a> JsonValue<'a> {
	fn fmtstr(s: &str) -> FmtCmd {
		let mut parts = vec![];
		let mut cur = "".to_string();
		for c in s.chars() {
			match c as i32 {
				0..=8 | 11..=31 | 127 => {
					let ctrlchar = (((c as i32 + 64) % 128) as u8 as char).to_string();
					parts.extend(vec![FmtCmd::lit(&cur), FmtCmd::exclude(FmtCmd::nobreak(FmtCmd::color(HI_KWD, FmtCmd::lit(&("^".to_string() + &ctrlchar)))))]);
					cur = "".to_string();
				},
				_ => cur.push(c),
			};
		}
		if cur.len() > 0 { parts.push(FmtCmd::lit(&cur)); }
		FmtCmd::cat(parts)
	}
	fn fmtkey(&self) -> FmtCmd {
		match self.parent {
			ParentType::Root => FmtCmd::exclude(FmtCmd::color(HI_MUT, FmtCmd::lit("root"))),
			ParentType::Object => FmtCmd::color(HI_KEY, Self::fmtstr(&self.key)),
			ParentType::Array => FmtCmd::exclude(FmtCmd::color(HI_MUT, Self::fmtstr(&self.key))),
		}
	}
	fn fmtval(&self) -> FmtCmd {
		match self.value {
			Value::String(s) => FmtCmd::color(HI_STR, Self::fmtstr(s)),
			Value::Number(n) => FmtCmd::color(HI_KWD, FmtCmd::lit(&n.to_string())),
			Value::Bool(b) => FmtCmd::color(HI_KWD, FmtCmd::lit(if *b { "true" } else { "false" })),
			Value::Object(items) => FmtCmd::exclude(FmtCmd::color(HI_KWD, FmtCmd::lit(if items.is_empty() { "{ }" } else { "{...}" }))),
			Value::Array(items) => FmtCmd::exclude(FmtCmd::color(HI_KWD, FmtCmd::lit(if items.is_empty() { "[ ]" } else { "[...]" }))),
			Value::Null => FmtCmd::color(HI_KWD, FmtCmd::lit("null")),
		}
	}
}

impl<'a> DispValue<'a> for JsonValue<'a> {
	fn placeholder(&self) -> FmtCmd {
		self.fmtkey()
	}
	fn content(&self) -> FmtCmd {
		match self.parent {
			ParentType::Root => self.fmtval(),
			_ => FmtCmd::cat(vec![self.fmtkey(), FmtCmd::exclude(FmtCmd::color(HI_MUT, FmtCmd::lit(": "))), self.fmtval()]),
		}
	}
	fn expandable(&self) -> bool {
		match *self.value {
			Value::Array(_) | Value::Object(_) => true,
			_ => false,
		}
	}
	fn children(&self) -> Vec<Box<DispValue<'a> + 'a>> {
		match self.value {
			Value::Array(items) =>
				items.iter().enumerate().map(|(i, v)| Box::new(JsonValue { key: i.to_string(), value: &v, parent: ParentType::Array }) as Box<DispValue>).collect(),
			Value::Object(items) =>
				items.iter().map(|(k, v)| Box::new(JsonValue { key: k.to_string(), value: &v, parent: ParentType::Object }) as Box<DispValue>).collect(),
			_ => vec![],
		}
	}
	fn invoke(&self) { }
}

pub struct JsonSource {
	json: Value,
}

impl JsonSource {
	pub fn read<T: std::io::Read>(input: T) -> Result<Box<Self>> {
		Ok(Box::new(Self { json: from_reader(input).chain_err(|| "could not parse input as JSON")? }))
	}
}

impl<'a> DispSource<'a, JsonValue<'a>> for JsonSource {
	fn root(&'a self) -> JsonValue<'a> {
		JsonValue { key: "root".to_string(), value: &self.json, parent: ParentType::Root }
	}
	fn colors(&self) -> Vec<Color> {
		vec![
			Color { c8: 2, c256: 77 }, // string
			Color { c8: 1, c256: 214 }, // keyword
			Color { c8: 5, c256: 177 }, // key
			Color { c8: 4, c256: 244 }, // muted
		]
	}
}
