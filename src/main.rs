use std::{collections::HashMap, env, ffi::OsStr, fs, io::Result, path::Path};

use titlecase::titlecase;
use walkdir::WalkDir;

const DEFAULT_INPUT_FOLDER: &str = "!sting_data";
const DEFAULT_CONFIG_FILE:  &str = "default_config.md";
const TEMPLATE_FILE:        &str = "template.html";

const TEMPLATE_REPLACE:     &str = "{!sting_replace}";

const CONFIG_SPLIT:   &str = "\n---\n";
const CONFIG_REPLACE: &str = "!sting_config_";

const STYLE_BOX:     &str = "{box}";
const STYLE_TITLE:   &str = "{title}";
const STYLE_BODY:    &str = "{body}";
const STYLE_END:     &str = "{end}";
const STYLE_END_BOX: &str = "{end-box}";

type ConfigMap = HashMap<String, String>;

// Simple helper function
fn warn_err<T>(r: Result<T>) {
    if let Err(e) = r {
        println!("{e}")
    }
}

// Holy shit this fucking sucks... but it works
trait ReplaceWithEscaping {
    fn replace_wih_escaping(self, from: &str, to: &str) -> Self;
}

impl ReplaceWithEscaping for String {
    fn replace_wih_escaping(self, from: &str, to: &str) -> Self {
        const HACKY_SOLUTION: &str = "!!}1==--HACKY-SOLUTION!!--==1{!!";
        self
            .replace(&format!("\\{from}")[..], HACKY_SOLUTION)
            .replace(from, to)
            .replace(HACKY_SOLUTION, from)
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let input_folder = Path::new(match args.get(1) {
        Some(i) => i.as_str(),
        None => DEFAULT_INPUT_FOLDER,
    });

    let template = match fs::read_to_string(input_folder.join(TEMPLATE_FILE)) {
        Ok(t)  => t,
        Err(_) => String::from(TEMPLATE_REPLACE),
    };
    let default_configs = fs::read_to_string(input_folder.join(DEFAULT_CONFIG_FILE))
        .map(|s| parse_configs(&s, None, None))
        .unwrap_or_default();
    let skip_paths = ["", TEMPLATE_FILE, DEFAULT_CONFIG_FILE]
        .map(|p| Path::new(p));

    for dir_entry in WalkDir::new(input_folder) {
        let dir_entry = match dir_entry {
            Ok(d)  => d,
            Err(e) => { println!("{e}"); continue; },
        };

        // The new file should be one folder above the input folder
        let out_path = match dir_entry.path().strip_prefix(input_folder) {
            // Make sure it can't make a recursive thingy!!
            Ok(p) if p.starts_with(input_folder) => { println!("{:?} has the same name as the input folder, skipping!", p.display()); continue; }
            Ok(p) if skip_paths.contains(&p) => continue,
            Ok(p)  => p,
            Err(e) => { println!("{e}"); continue; },
        };
        
        // If it's a directory, create it, unless it's the same as the input folder!
        if dir_entry.path().is_dir() && !out_path.exists() {
            warn_err(fs::create_dir(out_path));
            continue;
        }
        // If it's an index.md file, or a 404 in the root, parse it!
        if dir_entry.path().is_file() && (dir_entry.path().file_name() == Some(OsStr::new("index.md")) || dir_entry.path() == input_folder.join(Path::new("404.md"))) {
            let file_string = match fs::read_to_string(dir_entry.path()) {
                Ok(m)  => m,
                Err(e) => { println!("{e}"); continue; },
            };
            let page_path = out_path.with_extension("html");
            let parsed = parse(file_string, &template, &default_configs, &page_path);
            warn_err(fs::write(page_path, parsed));
            continue;
        }
        // If it's a file and not one parsed, just copy it over
        if dir_entry.path().is_file()  {
            warn_err(fs::copy(dir_entry.path(), out_path));
            continue;
        }
    }
}

fn parse(mut string: String, template: &String, default_configs: &ConfigMap, page_path: &Path) -> String {
    let page_parent = page_path.parent().unwrap_or(Path::new(""));

    let configs = get_configs(&mut string, default_configs, page_parent);
    // Parse the file and put it into the template
    let string = string
        .replace_wih_escaping(STYLE_BOX,     "<div class=\"box\">\n\n")
        .replace_wih_escaping(STYLE_TITLE,   "<div class=\"title\">\n\n")
        .replace_wih_escaping(STYLE_BODY,    "<div class=\"body\">\n\n")
        .replace_wih_escaping(STYLE_END,     "</div>")
        .replace_wih_escaping(STYLE_END_BOX, "</div>");
    let markdown_options = markdown::Options {
        compile: markdown::CompileOptions {
            allow_dangerous_html: true,
            allow_dangerous_protocol: true,
            ..markdown::CompileOptions::default()
        },
        ..markdown::Options::default()
    };
    let parsed = match markdown::to_html_with_options(&string, &markdown_options) {
        Ok(p)  => format!("<base href=\"/{}/\">{p}", page_path.parent().unwrap_or(Path::new("")).display()),
        Err(e) => return format!("{e}"),
    };
    let mut parsed = template.replacen(TEMPLATE_REPLACE, &parsed, 1);
    // Replace the config values in the parsed html file
    for (config, value) in configs.iter() {
        parsed = parsed.replace(&format!("{{{CONFIG_REPLACE}{config}}}"), value);
    }
    parsed
}

fn get_configs(string: &mut String, default_configs: &ConfigMap, page_parent: &Path) -> ConfigMap {
    // Split the string into the configs and the actual string
    let split: Vec<String> = string
        .splitn(2, CONFIG_SPLIT)
        .map(|s| String::from(s))
        .collect();

    let config_string = match split.get(0) {
        Some(c) if string.contains(CONFIG_SPLIT) => c,
        _ => return default_configs.clone(),
    };
    *string = match split.get(1) {
        Some(s) => String::from(s),
        None    => String::new(),
    };

    parse_configs(config_string, Some(default_configs), Some(page_parent))
}

fn parse_configs(config_string: &String, default_configs: Option<&ConfigMap>, page_parent: Option<&Path>) -> ConfigMap {
    let mut config_map = default_configs.map(|c| c.clone()).unwrap_or_default();
    if let Some(page_parent) = page_parent {
        config_map.insert("title".to_owned(), title(page_parent));
    }

    for line in config_string.split("\n") {
        let mut c = line.splitn(2, ": ");
        let config = match c.next() {
            Some(s) => s.to_string(),
            None    => continue,
        };
        let config_value = match c.next() {
            Some(s) => s.to_string(),
            None    => String::new(),
        };
        config_map.insert(config, config_value);
    }

    config_map
}

fn title(path: &Path) -> String {
    let string = path.file_name()
        .map(|f| f.to_string_lossy())
        .unwrap_or_default()
        .to_string();
    titlecase(&string.replace('_', " ").replace('-', " "))
}