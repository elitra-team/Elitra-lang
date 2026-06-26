use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const VERSION: &str = "0.1.0";

struct PackageMeta {
    name: String,
    version: String,
    description: String,
    entry: String,
    deps: HashMap<String, String>,
}

fn parse_package_toml(content: &str) -> Result<PackageMeta, String> {
    let mut meta = PackageMeta {
        name: String::new(),
        version: String::new(),
        description: String::new(),
        entry: "src/main.eltr".to_string(),
        deps: HashMap::new(),
    };
    let mut in_deps = false;
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[dependencies]" {
            in_deps = true;
            continue;
        }
        if line.starts_with('[') {
            in_deps = false;
            continue;
        }
        if in_deps {
            if let Some((name, ver)) = line.split_once('=') {
                let k = name.trim().to_string();
                let v = ver.trim().trim_matches('"').trim_matches('\'').to_string();
                if !k.is_empty() {
                    meta.deps.insert(k, v);
                }
            }
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let k = key.trim();
            let v = value.trim().trim_matches('"');
            match k {
                "name" => meta.name = v.to_string(),
                "version" => meta.version = v.to_string(),
                "description" => meta.description = v.to_string(),
                "entry" => meta.entry = v.to_string(),
                _ => {}
            }
        }
    }
    if meta.name.is_empty() {
        return Err("package.toml missing 'name'".into());
    }
    Ok(meta)
}

fn fmt_package_toml(meta: &PackageMeta) -> String {
    let mut s = String::new();
    s.push_str(&format!("name = \"{}\"\n", meta.name));
    s.push_str(&format!("version = \"{}\"\n", meta.version));
    if !meta.description.is_empty() {
        s.push_str(&format!("description = \"{}\"\n", meta.description));
    }
    s.push_str(&format!("entry = \"{}\"\n", meta.entry));
    if !meta.deps.is_empty() {
        s.push_str("\n[dependencies]\n");
        let mut deps: Vec<_> = meta.deps.iter().collect();
        deps.sort_by(|a, b| a.0.cmp(b.0));
        for (name, ver) in deps {
            s.push_str(&format!("{} = \"{}\"\n", name, ver));
        }
    }
    s
}

fn cmd_init(args: &[String]) {
    let raw = args.first().cloned().unwrap_or_else(|| {
        let cwd = std::env::current_dir().ok();
        cwd.as_ref()
            .and_then(|d| d.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("project")
            .to_string()
    });

    let dir = Path::new(&raw);
    let name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&raw)
        .to_string();
    if dir.exists() {
        eprintln!("error: '{}' already exists", name);
        std::process::exit(1);
    }

    fs::create_dir_all(dir.join("src")).unwrap_or_else(|e| {
        eprintln!("error: could not create src/: {}", e);
        std::process::exit(1);
    });

    let meta = PackageMeta {
        name: name.clone(),
        version: "0.1.0".to_string(),
        description: String::new(),
        entry: "src/main.eltr".to_string(),
        deps: HashMap::new(),
    };

    let toml_content = fmt_package_toml(&meta);
    fs::write(dir.join("package.toml"), &toml_content).unwrap_or_else(|e| {
        eprintln!("error: could not write package.toml: {}", e);
        std::process::exit(1);
    });

    let main_content = "shout \"Hello from new project!\"\n";
    fs::write(dir.join("src/main.eltr"), main_content).unwrap_or_else(|e| {
        eprintln!("error: could not write src/main.eltr: {}", e);
        std::process::exit(1);
    });

    println!("Created project '{}'", name);
    println!("  package.toml");
    println!("  src/main.eltr");
}

fn find_package_toml() -> Result<PathBuf, String> {
    let candidates = ["package.toml", "Package.toml"];
    for c in &candidates {
        let p = Path::new(c);
        if p.exists() {
            return Ok(p.to_path_buf());
        }
    }
    Err("no package.toml found in current directory".into())
}

fn cmd_run() {
    let path = match find_package_toml() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };
    let content = fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("error: reading package.toml: {}", e);
        std::process::exit(1);
    });
    let meta = match parse_package_toml(&content) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };
    let entry = Path::new(&meta.entry);
    if !entry.exists() {
        eprintln!("error: entry point '{}' not found", meta.entry);
        std::process::exit(1);
    }

    let status = Command::new("elitra")
        .arg(entry)
        .status()
        .unwrap_or_else(|e| {
            eprintln!("error: could not run 'elitra': {}", e);
            std::process::exit(1);
        });

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
}

fn cmd_build() {
    let path = match find_package_toml() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };
    let content = fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("error: reading package.toml: {}", e);
        std::process::exit(1);
    });
    let meta = match parse_package_toml(&content) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };
    let entry = Path::new(&meta.entry);
    if !entry.exists() {
        eprintln!("error: entry point '{}' not found", meta.entry);
        std::process::exit(1);
    }

    let status = Command::new("elitra")
        .arg(&meta.entry)
        .arg("--check-types")
        .status()
        .unwrap_or_else(|e| {
            eprintln!("error: could not run 'elitra': {}", e);
            std::process::exit(1);
        });

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    println!("Build OK");
}

fn cmd_install(args: &[String]) {
    if args.is_empty() {
        eprintln!("error: 'eltr install' requires a package name");
        std::process::exit(1);
    }

    let path = match find_package_toml() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };
    let content = fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("error: reading package.toml: {}", e);
        std::process::exit(1);
    });
    let mut meta = match parse_package_toml(&content) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    for pkg in args {
        if meta.deps.contains_key(pkg) {
            println!("'{}' already installed", pkg);
            continue;
        }
        let packages_dir = Path::new("packages");
        if !packages_dir.exists() {
            fs::create_dir_all(packages_dir).unwrap_or_else(|e| {
                eprintln!("error: creating packages/ dir: {}", e);
                std::process::exit(1);
            });
        }
        let pkg_dir = packages_dir.join(pkg);
        if !pkg_dir.exists() {
            fs::create_dir_all(&pkg_dir).unwrap_or_else(|e| {
                eprintln!("error: creating package dir: {}", e);
                std::process::exit(1);
            });
        }

        meta.deps.insert(pkg.clone(), "*".to_string());
        println!("Installed '{}'", pkg);
    }

    let toml_content = fmt_package_toml(&meta);
    fs::write(&path, &toml_content).unwrap_or_else(|e| {
        eprintln!("error: writing package.toml: {}", e);
        std::process::exit(1);
    });
}

fn print_help() {
    println!("eltr v{} - Elitra package manager", VERSION);
    println!();
    println!("Usage: eltr [COMMAND] [OPTIONS]");
    println!();
    println!("Commands:");
    println!("  init [name]     Create a new project (default: current dir name)");
    println!("  run             Run the current project");
    println!("  build           Check the current project");
    println!("  install <pkg>   Install a package");
    println!("  --help, -h      Show this help");
    println!("  --version, -v   Show version");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() == 1 {
        print_help();
        return;
    }

    let cmd = &args[1];
    let rest: Vec<String> = args.iter().skip(2).cloned().collect();

    match cmd.as_str() {
        "init" => cmd_init(&rest),
        "run" => cmd_run(),
        "build" => cmd_build(),
        "install" => cmd_install(&rest),
        "--help" | "-h" => print_help(),
        "--version" | "-v" => println!("eltr v{}", VERSION),
        _ => {
            eprintln!("error: unknown command '{}'", cmd);
            eprintln!("Try 'eltr --help' for usage");
            std::process::exit(1);
        }
    }
}
