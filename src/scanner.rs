use std::path::Path;
use walkdir::WalkDir;
use anyhow::Result;
use crate::models::{RepoContext, SourceFile, Language};

// Files/dirs to always skip
const SKIP_DIRS: &[&str] = &[
    "target", "node_modules", ".git", ".github", "dist",
    "build", "__pycache__", ".venv", "venv",
];

const SKIP_FILES: &[&str] = &[
    ".DS_Store", "Thumbs.db", "*.lock", "*.sum",
];

// Max file size to read (50KB) — avoids reading huge generated files
const MAX_FILE_BYTES: u64 = 50_000;

// Max source files to include in context (keeps LLM prompt reasonable)
const MAX_SOURCE_FILES: usize = 20;

pub fn scan_local(repo_path: &str) -> Result<RepoContext> {
    let root = Path::new(repo_path).canonicalize()?;
    let repo_name = root
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    println!("   📁 Scanning: {}", root.display());

    let mut ctx = RepoContext {
        name: repo_name,
        ..Default::default()
    };

    for entry in WalkDir::new(&root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !should_skip(e.path()))
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        // Skip files that are too large
        if let Ok(meta) = path.metadata() {
            if meta.len() > MAX_FILE_BYTES {
                continue;
            }
        }

        let relative = path.strip_prefix(&root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        let filename = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();

        let ext = path
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();

        // README
        if filename.starts_with("readme") {
            ctx.readme = read_file(path).ok();
            println!("   ✓ README found");
            continue;
        }

        // AGENTS.md (existing)
        if filename == "agents.md" {
            ctx.existing_agents_md = read_file(path).ok();
            println!("   ✓ Existing AGENTS.md found");
            continue;
        }

        // Package manifests
        if matches!(filename.as_str(), "cargo.toml" | "package.json" | "pyproject.toml" | "go.mod") {
            ctx.package_manifest = read_file(path).ok();
            println!("   ✓ Package manifest found: {}", filename);
            continue;
        }

        // OpenAPI spec
        if is_openapi(path) {
            ctx.openapi_spec = read_file(path).ok();
            println!("   ✓ OpenAPI spec found: {}", relative);
            continue;
        }

        // Source files
        if is_source_file(&ext) && ctx.source_files.len() < MAX_SOURCE_FILES {
            if let Ok(content) = read_file(path) {
                let lang = Language::from_extension(&ext);
                ctx.source_files.push(SourceFile {
                    path: relative.clone(),
                    language: lang,
                    content,
                });
            }
        }
    }

    println!(
        "   ✓ Scan complete — {} source files collected",
        ctx.source_files.len()
    );

    Ok(ctx)
}

fn should_skip(path: &Path) -> bool {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();

    // Skip hidden dirs (but not hidden files at root level like .env)
    if name.starts_with('.') && path.is_dir() {
        return true;
    }

    SKIP_DIRS.iter().any(|d| name.as_ref() == *d)
        || SKIP_FILES.iter().any(|f| {
            if f.starts_with('*') {
                name.ends_with(&f[1..])
            } else {
                name.as_ref() == *f
            }
        })
}

fn is_source_file(ext: &str) -> bool {
    matches!(ext, "rs" | "py" | "ts" | "js" | "go" | "java" | "cpp" | "c" | "h")
}

fn is_openapi(path: &Path) -> bool {
    let name = path.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
    name.contains("openapi") || name.contains("swagger")
}

fn read_file(path: &Path) -> Result<String> {
    Ok(std::fs::read_to_string(path)?)
}
