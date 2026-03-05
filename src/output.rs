use owo_colors::OwoColorize;
use std::fmt::Display;

pub fn ok(text: &str) -> String {
  text.green().to_string()
}

pub fn warn(text: &str) -> String {
  text.yellow().to_string()
}

pub fn fail(text: &str) -> String {
  text.red().to_string()
}

pub fn info(text: &str) -> String {
  text.cyan().to_string()
}

pub fn strong(text: &str) -> String {
  text.bold().to_string()
}

pub fn section(title: &str) {
  println!("{}", title.bold());
}

pub fn item(label: &str, value: impl Display) {
  println!("- {}: {}", label, value);
}

pub fn note(message: impl Display) {
  println!("- {}", message);
}
