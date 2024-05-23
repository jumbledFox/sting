use std::{collections::HashMap, env, ffi::OsStr, fs, io::Result, path::Path};

use titlecase::titlecase;
use walkdir::WalkDir;

const DEFAULT_INPUT_FOLDER: &str = "!sting_data";
const TEMPLATE_REPLACE:     &str = "[!sting_replace]";
const TEMPLATE_BREADCRUMBS: &str = "[!sting_breadcrumbs]";
const CONFIG_SPLIT:         &str = "\n---\n";
const STYLE_BOX:     &str = "[[box]]";
const STYLE_TITLE:   &str = "[[title]]";
const STYLE_BODY:    &str = "[[body]]";
const STYLE_END:     &str = "[[end]]";
const STYLE_END_BOX: &str = "[[end-box]]";

type ConfigMap = HashMap<String, String>;

// Simple helper function
fn warn_err<T>(r: Result<T>) {
    if let Err(e) = r {
        println!("{e}")
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let input_folder = Path::new(match args.get(1) {
        Some(i) => i.as_str(),
        None => DEFAULT_INPUT_FOLDER,
    });

    let template = match fs::read_to_string(input_folder.join("template.html")) {
        Ok(t)  => t,
        Err(_) => String::from(TEMPLATE_REPLACE),
    };
    let default_configs = fs::read_to_string(input_folder.join("default_configs.md"))
        .map(|s| parse_configs(&s, None, None))
        .unwrap_or_default();

    for dir_entry in WalkDir::new(input_folder) {
        let dir_entry = match dir_entry {
            Ok(d)  => d,
            Err(e) => { println!("{e}"); continue; },
        };

        // The new file should be one folder above the input folder
        let out_path = match dir_entry.path().strip_prefix(input_folder) {
            // Make sure it can't make a recursive thingy!!
            Ok(p) if p.starts_with(input_folder) => { println!("Recursive thingy!!"); continue; }
            // If it's a blank path (first walkdir), or it's the template, don't bother!!
            Ok(p) if [Path::new(""), Path::new("template.html"), Path::new("default_configs.md")].contains(&p) => continue,
            Ok(p)  => p,
            Err(e) => { println!("{e}"); continue; },
        };
        
        // If it's a directory, create it, unless it's the same as the input folder!
        if dir_entry.path().is_dir() && !out_path.exists() {
            warn_err(fs::create_dir(out_path));
            continue;
        }
        // If it's a file that's NOT index.md, just copy it over
        if dir_entry.path().is_file() && dir_entry.path().file_name() != Some(OsStr::new("index.md")) {
            warn_err(fs::copy(dir_entry.path(), out_path));
            continue;
        }
        // If it's an index.md file, parse it!
        if dir_entry.path().is_file() && dir_entry.path().file_name() == Some(OsStr::new("index.md")) {
            let file_string = match fs::read_to_string(dir_entry.path()) {
                Ok(m)  => m,
                Err(e) => { println!("{e}"); continue; },
            };
            let page_path = out_path.with_extension("html");
            let parsed = parse(file_string, &template, &default_configs, &page_path);
            warn_err(fs::write(page_path, parsed))
        }
    }
}

fn parse(mut string: String, template: &String, default_configs: &ConfigMap, page_path: &Path) -> String {
    let page_parent = page_path.parent().unwrap_or(Path::new(""));

    let configs = get_configs(&mut string, default_configs, page_parent);
    // Parse the file and put it into the template
    let string = string
        .replace(STYLE_BOX,     "<div class=\"box\">\n\n")
        .replace(STYLE_TITLE,   "<div class=\"title\">\n\n")
        .replace(STYLE_BODY,    "<div class=\"body\">\n\n")
        .replace(STYLE_END,     "</div>")
        .replace(STYLE_END_BOX, "</div>");
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
    let mut parsed = template
        .replacen(TEMPLATE_REPLACE, &parsed, 1)
        .replacen(TEMPLATE_BREADCRUMBS, &generate_breadcrumbs(page_path, default_configs), 1);
    // Replace the config values in the parsed html file
    for (config, value) in configs.iter() {
        parsed = parsed.replace(&format!("[sting_{config}]"), value);
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

fn generate_breadcrumbs(page_path: &Path, default_configs: &ConfigMap) -> String {
    // Put in a closure so i only compute when necessary
    let root_message = || {
        let r = default_configs
            .get("breadcrumbs_root_message")
            .map(|s| s.clone())
            .unwrap_or_default();
        format!("<ul class=\"breadcrumb\"><li>{r}</li></ul>")
    };
    
    // If the page is the root, display the root message
    let p = match page_path.parent() {
        Some(p) if p != Path::new("") => p,
        _ => return root_message(),
    };

    let mut breadcrumbs = String::new();
    let mut current = p;
    let mut first = true;

    loop {
        // Add the current page to the breadcrumbs
        let mut crumb = format!("{}", title(current));
        // Don't add the link if it's the first breadcrumb
        if !first {
            let link = format!("/{}", current.display());
            crumb = format!("<a href=\"{link}\">{crumb}</a>")
        }
        first = false;
        breadcrumbs = format!("<li>{crumb}</li> {breadcrumbs}");
        // Set current to parent, unless it doesn't have a parent (or it's "")
        current = match current.parent() {
            Some(parent) if parent != Path::new("") => parent,
            _ => break
        };
    }
    let home_crumb = String::from("<li><a href=\"/\">Home</a></li>");
    format!("<ul class=\"breadcrumb\">{home_crumb}{breadcrumbs}</ul>")
}

fn title(path: &Path) -> String {
    let string = path.file_name()
        .map(|f| f.to_string_lossy())
        .unwrap_or_default()
        .to_string();
    titlecase(&string.replace('_', " ").replace('-', " "))
}