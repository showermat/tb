use ::interface::Value;
use ::interface::Format;

pub struct StatMsg {
	msg: String,
	color: usize
}

impl StatMsg {
	pub fn new(msg: String, color: usize ) -> Self {
		StatMsg { msg: msg, color: color }
	}
}

impl<'a> Value<'a> for StatMsg {
	fn content(&self) -> Format {
		Format::RawColor(self.color, Box::new(Format::Literal(self.msg.to_string())))
	}
	
	fn expandable(&self) -> bool { false }

	fn children(&self) -> Vec<Box<dyn Value<'a> + 'a>> { vec![] }
}
