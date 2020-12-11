use ::interface::fmt;

fn fmtstr(s: &str, ctrlcolor: usize) -> ::interface::Format {
	let mut parts = vec![];
	let mut cur = "".to_string();
	for c in s.chars() {
		match c as i32 {
			0..=8 | 11..=31 | 127 => {
				let ctrlchar = (((c as i32 + 64) % 128) as u8 as char).to_string();
				parts.extend(vec![fmt::lit(&cur), fmt::nosearch(fmt::nobreak(fmt::color(ctrlcolor, fmt::lit(&("^".to_string() + &ctrlchar)))))]);
				cur = "".to_string();
			},
			_ => cur.push(c),
		};
	}
	if cur.len() > 0 { parts.push(fmt::lit(&cur)); }
	fmt::cat(parts)
}

pub mod json;
pub mod fs;
pub mod txt;
