extern crate thiserror;

use nom::branch::alt;
use nom::bytes::complete::*;
use nom::character::complete::*;
use nom::combinator::*;
use nom::multi::*;
use nom::number::complete::*;
use nom::sequence::*;
use nom::Finish;
use nom::IResult;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
	#[error("couldn't parse input: {0}")]
	Parse(String),
}

impl From<nom::error::Error<&str>> for Error {
	fn from(error: nom::error::Error<&str>) -> Self {
		Self::Parse(error.to_string())
	}
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, PartialEq)]
pub enum Value {
	Int(i64),
	Float(f64),
	String(String),
	Enum(String),
	Message(Vec<(String, Box<Value>)>),
}

fn end_value(i: &str) -> IResult<&str, ()> {
	peek(
		alt((
			map(tag("}"), |_| ()),
			map(eof, |_| ()),
			required_space,
		))
	)(i)
}

fn int(i: &str) -> IResult<&str, Value> {
	let (i, ret) = tuple((
		opt(tag("-")),
		alt((
			map( // Hexadecimal
				preceded(alt((tag("0x"), tag("0X"))), many1(one_of("0123456789abcdefABCDEF"))),
				|x| (x, 16)
			),
			map( // Octal
				preceded(tag("0"), many1(one_of("01234567"))),
				|x| (x, 8)
			),
			map( // Decimal
				many1(one_of("0123456789")),
				|x| (x, 10)
			),
		)),
		end_value,
	))(i)?;
	let raw_int = i64::from_str_radix(&ret.1.0.into_iter().collect::<String>(), ret.1.1).expect("Recognized int was not an int");
	let negated = match ret.0 {
		Some(_) => -raw_int,
		None => raw_int,
	};
	Ok((i, Value::Int(negated)))
}

fn float(i: &str) -> IResult<&str, Value> {
	map(
		terminated(
			alt((
				map(tuple((opt(tag("+")), tag_no_case("inf"))), |_| f64::INFINITY),
				map(tag_no_case("-inf"), |_| f64::NEG_INFINITY),
				map(tag_no_case("nan"), |_| f64::NAN),
				map(recognize_float, |x: &str| x.parse::<f64>().expect("Recognized float was not a float")),
			)),
			end_value,
		),
		|x| Value::Float(x),
	)(i)
}

fn bareword(i: &str) -> IResult<&str, String> {
	map(
		tuple((
			one_of("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_"),
			many0(one_of("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_0123456789")),
		)),
		|(head, tail)| {
			let mut ret = String::new();
			ret.push(head);
			ret.push_str(&tail.into_iter().collect::<String>());
			ret
		}
	)(i)
}

fn enumer(i: &str) -> IResult<&str, Value> {
	map(bareword, Value::Enum)(i)
}

fn escaped_char(i: &str) -> IResult<&str, char> {
	preceded(
		tag("\\"),
		alt((
			one_of("\\\"'"),
			map(tag("a"), |_| '\x07'),
			map(tag("b"), |_| '\x08'),
			map(tag("t"), |_| '\t'),
			map(tag("n"), |_| '\n'),
			map(tag("v"), |_| '\x0b'),
			map(tag("f"), |_| '\x0c'),
			map(tag("r"), |_| '\r'),
			map( // Octal escape
				count(one_of("01234567"), 3),
				|x| {
					let s = x.into_iter().collect::<String>();
					let codepoint = u32::from_str_radix(&s, 8).expect("Octal was invalid int");
					char::from_u32(codepoint).unwrap_or('�')
				},
			),
			map( // Hex escape
				preceded(
					tag("x"),
					count(one_of("0123456789abcdefABCDEF"), 2),
				),
				|x| {
					let s = x.into_iter().collect::<String>();
					let codepoint = u32::from_str_radix(&s, 16).expect("Hex was invalid int");
					char::from_u32(codepoint).unwrap_or('�')
				},
			),
		)),
	)(i)
}

fn string(i: &str) -> IResult<&str, String> {
	map(
		alt((
			delimited(
				tag("\""),
				many0(
					alt((
						none_of("\\\""),
						escaped_char,
					)),
				),
				tag("\"")
			),
			delimited(
				tag("'"),
				many0(
					alt((
						none_of("\\'"),
						escaped_char,
					)),
				),
				tag("'")
			),
		)),
		|x| x.into_iter().collect::<String>(),
	)(i)
}

fn comment(i: &str) -> IResult<&str, String> {
	map(
		delimited(
			tuple((tag("#"), space0)),
			many0(none_of("\n")),
			tag("\n")
		),
		|x| x.into_iter().collect::<String>(),
	)(i)
}

fn optional_space(i: &str) -> IResult<&str, ()> {
	map(many0(alt((map(comment, |_| ""), multispace1))), |_| ())(i)
}

fn required_space(i: &str) -> IResult<&str, ()> {
	map(many1(alt((map(comment, |_| ""), multispace1))), |_| ())(i)
}

fn key_value(i: &str) -> IResult<&str, (String, Box<Value>)> {
	map(
		tuple((
			alt((
				bareword,
				delimited(
					tuple((tag("["), optional_space)),
					map(
						many1(one_of("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_0123456789./")),
						|x| format!("[{}]", x.into_iter().collect::<String>()),
					),
					tuple((optional_space, tag("]"))),
				),
			)),
			alt((
				preceded(
					tuple((optional_space, tag(":"), optional_space)),
					alt((
						int,
						float,
						enumer,
						map(
							separated_list1(required_space, string),
							|x| Value::String(x.into_iter().collect::<String>()),
						),
					)),
				),
				preceded(
					tuple((
						optional_space,
						opt(tuple((tag(":"), optional_space))),
					)),
					alt((
						delimited(tag("{"), message, tag("}")),
						delimited(tag("<"), message, tag(">")),
					)),
				),
			)),
		)),
		|x| (x.0, Box::new(x.1)),
	)(i)
}

fn message(i: &str) -> IResult<&str, Value> {
	map(
		delimited(
			optional_space,
			separated_list0(required_space, key_value),
			optional_space,
		),
		Value::Message,
	)(i)
}

fn file(i: &str) -> IResult<&str, Value> {
	all_consuming(
		terminated(
			delimited(
				optional_space,
				alt((
					delimited(tag("{"), message, tag("}")),
					delimited(tag("<"), message, tag(">")),
					message,
				)),
				optional_space,
			),
			eof
		)
	)(i)
}

pub fn parse(s: &str) -> Result<Value> {
	Ok(file(s).finish()?.1)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_int() {
		assert_eq!(int("123").unwrap(), ("", Value::Int(123)));
	}

	#[test]
	fn test_float() {
		let tests = vec![
			("2", 2.0),
			("2.0", 2.0),
			("2.05", 2.05),
			("2.05e2", 205.0),
			("2.3e-2", 0.023),
			("-2.5E2", -250.0),
			("+2.5e+2", 250.0),
		];
		for (s, v) in tests {
			assert_eq!(float(s).unwrap(), ("", Value::Float(v)));
		}
	}

	#[test]
	fn test_comment() {
		let res = comment("# test test\nhi").unwrap();
		assert_eq!(res, ("hi", "test test".to_string()));
	}

	#[test]
	fn test_key_value() {
		let res = key_value("test: FOO # comment").unwrap();
		assert_eq!(res.0, " # comment");
		assert_eq!(res.1.0, "test");
		match *res.1.1 {
			Value::Enum(x) => assert_eq!(x, "FOO"),
			_ => panic!("Not enum"),
		};
	}

	#[test]
	fn test_message() {
		let res = message("a: 1\na: 2\nb: 3\n???").unwrap();
		assert_eq!(res.0, "???");
		assert_eq!(res.1, Value::Message(vec![
			("a".to_string(), Box::new(Value::Int(1))),
			("a".to_string(), Box::new(Value::Int(2))),
			("b".to_string(), Box::new(Value::Int(3))),
		]));
	}

	#[test]
	fn test_nested_message() {
		let res = message("a { x: 1 } a { x: 2 z { z: 4 } } b{y:3} x").unwrap();
		assert_eq!(res.0, "x");
		assert_eq!(res.1, Value::Message(vec![
			("a".to_string(), Box::new(Value::Message(vec![
				("x".to_string(), Box::new(Value::Int(1))),
			]))),
			("a".to_string(), Box::new(Value::Message(vec![
				("x".to_string(), Box::new(Value::Int(2))),
				("z".to_string(), Box::new(Value::Message(vec![
					("z".to_string(), Box::new(Value::Int(4))),
				]))),
			]))),
			("b".to_string().to_string(), Box::new(Value::Message(vec![
				("y".to_string(), Box::new(Value::Int(3))),
			]))),
		]));
	}

	#[test]
	fn test_commented_message() {
		let res = message("a: { x: 1 # Hello\n} b{#Hi\n y:2 }?").unwrap();
		assert_eq!(res.0, "?");
		assert_eq!(res.1, Value::Message(vec![
			("a".to_string(), Box::new(Value::Message(vec![
				("x".to_string(), Box::new(Value::Int(1))),
			]))),
			("b".to_string(), Box::new(Value::Message(vec![
				("y".to_string(), Box::new(Value::Int(2))),
			]))),
		]));
	}

	#[test]
	fn test_type_annotation() {
		let res = message("a { [x.y.z] { b: FOO } }").unwrap();
		assert_eq!(res.0, "");
		assert_eq!(res.1, Value::Message(vec![
			("a".to_string(), Box::new(Value::Message(vec![
				("[x.y.z]".to_string(), Box::new(Value::Message(vec![
					("b".to_string(), Box::new(Value::Enum("FOO".to_string()))),
				]))),
			]))),
		]));
	}

	#[test]
	fn test_string() {
		let res = message("a: 'a\"s\\'d' b: \"q'w\\\"e\" c: \"\\\\\"").unwrap();
		//let res = message("a: \"a\"").unwrap();
		assert_eq!(res.0, "");
		assert_eq!(res.1, Value::Message(vec![
			("a".to_string(), Box::new(Value::String("a\"s'd".to_string()))),
			("b".to_string(), Box::new(Value::String("q'w\"e".to_string()))),
			("c".to_string(), Box::new(Value::String("\\".to_string()))),
		]));
	}

	//#[test]
	fn test_parse() {
		let input = r#"
test {
	# Comment
	int: 1
	int: 42
	int: 0x42
	int: 0420
	float # Comment
		: # Comment
		1 # comment
	#float: 2.0
	#float: 3.14159
	#float: 2.54E6

# Multiple lines
# of comments

	enum: UP
	a: 1 b: 2 c: 3
}
test2 {
	enum: DOWN
}
"#;
		println!("{:#?}", parse(input))
		// TODO
	}
}
