use ::tb_interface::*;
use ::errors::*;
use ::serde_json::Value as V;
use rayon::prelude::*;

// TODO filter_maps that silently ignore errors need to be done better!

#[derive(Clone)]
pub enum Content {
	Story { title: String, url: String, score: usize, descendants: usize },
	Comment { parent: usize, text: String },
}

#[derive(Clone)]
pub struct Post {
	by: String,
	id: usize,
	time: usize,
	content: Content,
}

#[derive(Clone)]
pub struct Item {
	kids: Vec<usize>,
	post: Option<Post>,
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
		let content = match &get_as::<String>(&raw, "type")? as &str {
			"story" => Content::Story {
				title: get_as::<String>(&raw, "title")?,
				url: get_as::<String>(&raw, "url")?,
				score: get_as::<usize>(&raw, "score")?,
				descendants: get_as::<usize>(&raw, "descendants")?,
			},
			"comment" => Content::Comment {
				parent: get_as::<usize>(&raw, "parent")?,
				text: get_as::<String>(&raw, "text")?,
			},
			kind => bail!("Unknown post type {}", kind),
		};
		let post = Post {
			by: get_as::<String>(&raw, "by")?,
			id: get_as::<usize>(&raw, "id")?,
			time: get_as::<usize>(&raw, "time")?,
			content: content,
		};
		Ok(Self { post: Some(post), kids: get_as::<Vec<usize>>(&raw, "kids")? })
	}

	fn topstories() -> Result<Self> {
		const MAX: usize = 50;
		let mut top: Vec<usize> = serde_json::from_value(Self::hnjson("topstories")?).chain_err(|| "Couldn't get list of top stories")?;
		top.truncate(MAX);
		Ok(Self { post: None, kids: top })
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
		if let Some(post) = &self.post {
			match &post.content {
				Content::Story { title, url, score, descendants } => Format::cat(vec![
					Format::color(0, Format::lit(&title)),
					Format::lit("\n"),
					Format::lit(&url),
					Format::lit("\n"),
					Format::color(1, Format::lit(&format!("{} points by {} {} - {} comments", score, post.by, timefmt(post.time), descendants))),
				]),
				Content::Comment { text, .. } => Format::cat(vec![
					Format::color(1, Format::lit(&format!("{} {}\n", post.by, timefmt(post.time)))),
					Format::lit(&html2text::from_read(text.as_bytes(), 10090)),
				]),
			}
		}
		else {
			Format::lit("Hacker News")
		}
	}

	fn placeholder(&self) -> Format {
		self.content()
	}

	fn expandable(&self) -> bool {
		true
	}

	fn children(&self) -> Vec<Box<Value<'a> + 'a>> {
		let ret: Vec<Item> = self.kids.par_iter().filter_map(|id| Self::get(*id).ok()).collect();
		ret.into_iter().map(|x| Box::new(x) as Box<Value>).collect()
	}

	fn invoke(&self) {
		if let Some(post) = &self.post {
			if let Content::Story { url, .. } = &post.content {
				if let Ok(browser) = std::env::var("BROWSER") {
					let _ = std::process::Command::new(browser).arg(url).status();
				}
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
			None => Item::topstories()?,
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
