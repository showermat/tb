use ::tb_interface::*;
use ::tb_interface::fmt::*;
use ::errors::*;
use ::serde_json::Value as V;
use rayon::prelude::*;

#[derive(Clone)]
pub struct PostInfo {
	by: String,
	id: usize,
	time: usize,
}

#[derive(Clone)]
pub enum Item {
	Root,
	Story { title: String, url: String, score: usize, descendants: usize, info: PostInfo },
	Comment { parent: usize, text: String, info: PostInfo },
	//Error { msg: String },
}

impl Item {
	fn hnjson(arg: &str) -> Result<V> {
		let url = format!("https://hacker-news.firebaseio.com/v0/{}.json", arg);
		Ok(serde_json::from_reader(reqwest::get(&url).chain_err(|| format!("Failed to fetch {}", url))?).chain_err(|| format!("Could not interpret contents of {} as JSON", url))?)
	}

	fn get(id: usize) -> Result<Self> {
		fn get_as<T>(v: &V, key: &str) -> Result<T> where for<'de> T: serde::Deserialize<'de> {
			Ok(serde_json::from_value(v.get(key).chain_err(|| "Could not get item at key")?.clone()).chain_err(|| "Item at key was not of requested type")?)
		}
		let raw = Self::hnjson(&format!("item/{}", id))?;
		let info = PostInfo {
			by: get_as::<String>(&raw, "by")?,
			id: get_as::<usize>(&raw, "id")?,
			time: get_as::<usize>(&raw, "time")?,
		};
		Ok(match &get_as::<String>(&raw, "type")? as &str {
			"story" => Item::Story {
				title: get_as::<String>(&raw, "title")?,
				url: get_as::<String>(&raw, "url")?,
				score: get_as::<usize>(&raw, "score")?,
				descendants: get_as::<usize>(&raw, "descendants")?,
				info: info,
			},
			"comment" => Item::Comment {
				parent: get_as::<usize>(&raw, "parent")?,
				text: get_as::<String>(&raw, "text")?,
				info: info,
			},
			kind => bail!("Unknown post type {}", kind),
		})
	}

	fn childids(&self) -> Result<Vec<usize>> {
		match self {
			Item::Root => {
				const MAX: usize = 50;
				let mut ret: Vec<usize> = serde_json::from_value(Self::hnjson("topstories")?).chain_err(|| "Couldn't get list of top stories")?;
				ret.truncate(MAX);
				Ok(ret)
			},
			Item::Story { info, .. } | Item::Comment { info, .. } => {
				let jsonchildren = Self::hnjson(&format!("item/{}", info.id))?.get("kids").chain_err(|| "Could not get children")?.clone();
				Ok(serde_json::from_value(jsonchildren).chain_err(|| "Children were not of requested type")?)
			}
		}
	}
}

impl<'a> Value<'a> for Item {
	fn content(&self) -> Format {
		fn timefmt(timestamp: usize) -> String {
			use chrono::prelude::*;
			let f = timeago::Formatter::new();
			let date = Utc.timestamp(timestamp as i64, 0);
			f.convert_chrono(date, Utc::now())
		}
		match &self {
			Item::Root => lit("Hacker News"),
			Item::Story { title, url, score, descendants, info } => cat(vec![
				noyank(cat(vec![color(0, lit(&title)), lit("\n")])),
				lit(&url),
				noyank(color(1, lit(&format!("\n{} points by {} {} - {} comments", score, info.by, timefmt(info.time), descendants)))),
			]),
			Item::Comment { text, info, .. } => cat(vec![
				color(1, lit(&format!("{} {}\n", info.by, timefmt(info.time)))),
				lit(&html2text::from_read(text.as_bytes(), 10090)),
			]),
			//Item::Error { msg } => color(1, lit(msg))
		}
	}

	fn expandable(&self) -> bool {
		true
	}

	fn children(&self) -> Vec<Box<Value<'a> + 'a>> {
		let ids = self.childids().unwrap_or(vec![]);
		let ret: Vec<Item> = ids.par_iter().filter_map(|id| Self::get(*id).ok()).collect();
		ret.into_iter().map(|x| Box::new(x) as Box<Value>).collect()
	}

	fn invoke(&self) {
		if let Item::Story { url, .. } = &self {
			if let Ok(browser) = std::env::var("BROWSER") {
				let _ = std::process::Command::new(browser).arg(url).status();
			}
		}
	}
}

struct HnSource {
	root: Item
}

impl HnSource {
	fn new(id: Option<usize>) -> Result<Self> {
		let item = match id {
			Some(i) => Item::get(i)?,
			None => Item::Root,
		};
		Ok(Self { root: item })
	}
}

impl Source for HnSource {
	fn root<'a>(&'a self) -> Box<Value<'a> + 'a> {
		Box::new(self.root.clone())
	}
}

pub struct HnFactory { }

impl HnFactory {
	fn construct(args: &[&str]) -> Result<Box<Source>> {
		if args.len() > 1 {
			bail!("Only one argument is permitted");
		}
		else {
			let id = match args.get(0) {
				Some(i) => Some(i.parse::<usize>().chain_err(|| "Argument is not an integer")?),
				None => None,
			};
			Ok(Box::new(HnSource::new(id)?))
		}
	}
}

impl Factory for HnFactory {
	fn info(&self) -> Info {
		Info { name: "hn", desc: "Read HackerNews threads" }
	}

	fn from(&self, args: &[&str]) -> Option<Result<Box<Source>>> {
		Some(Self::construct(args))
	}

	fn colors(&self) -> Vec<Color> {
		vec![
			Color { c8: 2, c256: 2 }, // Headline
			Color { c8: 4, c256: 244 }, // muted
		]
	}
}
