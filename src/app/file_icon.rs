//! Which Catppuccin icon (assets/icons/files/) a file gets, like VS Code's icon theme:
//! special file names first (`Cargo.toml`, `package.json`, `.gitignore`…), then the
//! extension, then a generic file.

use iced::widget::{Svg, svg};

use crate::theme::layouts;

macro_rules! file_icons {
    ($($name:ident => $file:literal),* $(,)?) => {
        $(const $name: &[u8] = include_bytes!(concat!("../../assets/icons/files/", $file, ".svg"));)*
    };
}

file_icons! {
    FILE => "_file",
    RUST => "rust",
    CARGO => "cargo",
    TYPESCRIPT => "typescript",
    TYPESCRIPT_REACT => "typescript-react",
    JAVASCRIPT => "javascript",
    JAVASCRIPT_REACT => "javascript-react",
    JSON => "json",
    PACKAGE_JSON => "package-json",
    NPM => "npm",
    TOML => "toml",
    YAML => "yaml",
    MARKDOWN => "markdown",
    HTML => "html",
    CSS => "css",
    SHELL => "bash",
    IMAGE => "image",
    SVG => "svg",
    LOCK => "lock",
    GIT => "git",
    TEXT => "text",
    DOCKER => "docker",
    ENV => "env",
    PYTHON => "python",
    GO => "go",
    PRETTIER => "prettier",
    ESLINT => "eslint",
    XML => "xml",
    JAVA => "java",
    CSHARP => "csharp",
    C => "c",
    CPP => "cpp",
    VUE => "vue",
    SVELTE => "svelte",
}

/// The icon for a file, drawn in its own colors (no tint, unlike the UI icons).
/// 16px, the size Catppuccin draws them for: their 1px strokes sit on half-pixels of a
/// 16-unit grid, so any other size would blur them.
pub(super) fn file_icon<'a>(path: &str) -> Svg<'a> {
    svg(svg::Handle::from_memory(icon_for(path)))
        .width(layouts::FILE_ICON)
        .height(layouts::FILE_ICON)
}

fn icon_for(path: &str) -> &'static [u8] {
    let name = path.rsplit('/').next().unwrap_or(path);
    let lower = name.to_ascii_lowercase();

    if let Some(icon) = icon_for_name(&lower) {
        return icon;
    }

    let extension = lower.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("");
    icon_for_extension(extension)
}

fn icon_for_name(name: &str) -> Option<&'static [u8]> {
    let icon = match name {
        "cargo.toml" | "cargo.lock" => CARGO,
        "package.json" => PACKAGE_JSON,
        "package-lock.json" | "yarn.lock" | "pnpm-lock.yaml" | "bun.lockb" | "bun.lock" => LOCK,
        ".npmrc" | ".nvmrc" => NPM,
        ".gitignore" | ".gitattributes" | ".gitmodules" | ".gitkeep" => GIT,
        "dockerfile"
        | ".dockerignore"
        | "docker-compose.yml"
        | "docker-compose.yaml"
        | "compose.yml"
        | "compose.yaml" => DOCKER,
        ".env" => ENV,
        _ if name.starts_with(".env.") => ENV,
        _ if name.starts_with(".prettierrc") || name.starts_with("prettier.config.") => PRETTIER,
        _ if name.starts_with(".eslintrc") || name.starts_with("eslint.config.") => ESLINT,
        _ => return None,
    };
    Some(icon)
}

fn icon_for_extension(extension: &str) -> &'static [u8] {
    match extension {
        "rs" => RUST,
        "ts" | "mts" | "cts" => TYPESCRIPT,
        "tsx" => TYPESCRIPT_REACT,
        "js" | "mjs" | "cjs" => JAVASCRIPT,
        "jsx" => JAVASCRIPT_REACT,
        "json" | "jsonc" => JSON,
        "toml" => TOML,
        "yml" | "yaml" => YAML,
        "md" | "mdx" => MARKDOWN,
        "html" | "htm" => HTML,
        "css" | "scss" | "sass" | "less" => CSS,
        "sh" | "bash" | "zsh" | "fish" | "ps1" => SHELL,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "ico" | "bmp" | "avif" => IMAGE,
        "svg" => SVG,
        "lock" => LOCK,
        "txt" | "log" => TEXT,
        "py" => PYTHON,
        "go" => GO,
        "xml" => XML,
        "java" => JAVA,
        "cs" => CSHARP,
        "c" | "h" => C,
        "cpp" | "cc" | "cxx" | "hpp" | "hh" => CPP,
        "vue" => VUE,
        "svelte" => SVELTE,
        _ => FILE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_names_win_over_extensions() {
        assert_eq!(icon_for("Cargo.toml"), CARGO);
        assert_eq!(icon_for("web/package.json"), PACKAGE_JSON);
        assert_eq!(icon_for("web/package-lock.json"), LOCK);
        assert_eq!(icon_for(".env.local"), ENV);
        assert_eq!(icon_for("Dockerfile"), DOCKER);
    }

    #[test]
    fn test_extensions() {
        assert_eq!(icon_for("src/main.rs"), RUST);
        assert_eq!(icon_for("app/[id]/page.tsx"), TYPESCRIPT_REACT);
        assert_eq!(icon_for("config.TOML"), TOML);
        assert_eq!(icon_for("notes/ideas.txt"), TEXT);
    }

    #[test]
    fn test_unknown_and_extensionless_get_the_generic_icon() {
        assert_eq!(icon_for("LICENSE"), FILE);
        assert_eq!(icon_for("data.parquet"), FILE);
        assert_eq!(icon_for("newdir/"), FILE);
    }
}
