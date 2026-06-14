//! Design-system extraction: palette (`Theme::dark()` fields), icon glyphs
//! (`icon_pair()` registry), and a mascot summary become the `design_system`
//! block.
//!
//! Like [`super::features`], every parser is intentionally infallible — a
//! missing file or unparseable source yields an empty section rather than
//! aborting the scan. `styles` and `layout_conventions` are left empty by
//! design (a later follow-up may populate them).
//!
//! The issue's literal spec assumed `pub const NAME: Color` items; the real
//! code keeps colors as `Theme` struct fields set in the `dark()` constructor
//! and icon glyphs as `icon_pair()` match arms, so the extractors read those
//! shapes instead.

#[cfg(test)]
mod tests;

use crate::insight::schema::{DesignSystem, Mascot, NamedValue};

/// Extract palette entries from the `Theme::dark()` constructor in theme.rs
/// source. Reads the field assignments of the returned `Self { .. }` literal;
/// each becomes a [`NamedValue`] (field name + rendered `Color`). Named colors
/// render to their variant (`Color::Green` -> `"Green"`); `Color::Rgb(r,g,b)`
/// renders to `"rgb(r,g,b)"`. Unrecognized expressions are skipped, never
/// panic. Infallible: unparseable source yields an empty `Vec`.
pub fn palette_from_source(src: &str) -> Vec<NamedValue> {
    let Ok(file) = syn::parse_file(src) else {
        return Vec::new();
    };
    let Some(block) = find_dark_fn(&file) else {
        return Vec::new();
    };
    struct_fields_to_palette(block)
}

/// Locate the body of the `Theme::dark()` constructor in a parsed file.
fn find_dark_fn(file: &syn::File) -> Option<&syn::Block> {
    for item in &file.items {
        if let syn::Item::Impl(imp) = item {
            for impl_item in &imp.items {
                if let syn::ImplItem::Fn(f) = impl_item
                    && f.sig.ident == "dark"
                {
                    return Some(&f.block);
                }
            }
        }
    }
    None
}

/// Pull the field assignments out of the first struct literal in a block.
fn struct_fields_to_palette(block: &syn::Block) -> Vec<NamedValue> {
    for stmt in &block.stmts {
        if let syn::Stmt::Expr(syn::Expr::Struct(s), _) = stmt {
            return s.fields.iter().filter_map(field_to_named).collect();
        }
    }
    Vec::new()
}

/// One `field: Color::...` assignment into a [`NamedValue`], or `None` when the
/// field is positional or the color expression is unrecognized.
fn field_to_named(field: &syn::FieldValue) -> Option<NamedValue> {
    let name = match &field.member {
        syn::Member::Named(id) => id.to_string(),
        syn::Member::Unnamed(_) => return None,
    };
    let value = color_expr_to_string(&field.expr)?;
    Some(NamedValue { name, value })
}

/// Render a `Color` expression to a stable string. Total: anything it does not
/// recognize returns `None` (the field is skipped), never a panic.
fn color_expr_to_string(expr: &syn::Expr) -> Option<String> {
    match expr {
        // `Color::Green` -> "Green"
        syn::Expr::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
        // `Color::Rgb(r, g, b)` -> "rgb(r,g,b)"
        syn::Expr::Call(c) => {
            let is_rgb = matches!(
                &*c.func,
                syn::Expr::Path(p)
                    if p.path.segments.last().is_some_and(|s| s.ident == "Rgb")
            );
            if !is_rgb {
                return None;
            }
            let nums: Vec<String> = c.args.iter().filter_map(int_lit).collect();
            if nums.len() != 3 {
                return None;
            }
            Some(format!("rgb({},{},{})", nums[0], nums[1], nums[2]))
        }
        _ => None,
    }
}

/// The base-10 digits of an integer-literal expression, else `None`.
fn int_lit(expr: &syn::Expr) -> Option<String> {
    if let syn::Expr::Lit(l) = expr
        && let syn::Lit::Int(i) = &l.lit
    {
        return Some(i.base10_digits().to_string());
    }
    None
}

/// Extract icon glyphs from the `icon_pair()` match in icons.rs source. Each
/// `IconId::Name => IconPair::new("nerd", _)` arm becomes a [`NamedValue`]
/// (variant name + nerd glyph). Arms whose body is not an `IconPair::new(..)`
/// call are skipped. Infallible: unparseable source yields an empty `Vec`.
pub fn icons_from_source(src: &str) -> Vec<NamedValue> {
    let Ok(file) = syn::parse_file(src) else {
        return Vec::new();
    };
    for item in &file.items {
        if let syn::Item::Fn(f) = item
            && f.sig.ident == "icon_pair"
        {
            return match_arms_to_icons(&f.block);
        }
    }
    Vec::new()
}

/// Pull `(variant, glyph)` pairs out of the first match expression in a block.
fn match_arms_to_icons(block: &syn::Block) -> Vec<NamedValue> {
    for stmt in &block.stmts {
        if let syn::Stmt::Expr(syn::Expr::Match(m), _) = stmt {
            return m.arms.iter().filter_map(arm_to_icon).collect();
        }
    }
    Vec::new()
}

/// One match arm into a [`NamedValue`], or `None` when the pattern is not a
/// path or the body is not an `IconPair::new("glyph", ..)` call.
fn arm_to_icon(arm: &syn::Arm) -> Option<NamedValue> {
    let name = pat_last_ident(&arm.pat)?;
    let value = icon_pair_first_str(&arm.body)?;
    Some(NamedValue { name, value })
}

/// Last path segment of a pattern like `IconId::ChevronRight`.
fn pat_last_ident(pat: &syn::Pat) -> Option<String> {
    if let syn::Pat::Path(p) = pat {
        return p.path.segments.last().map(|s| s.ident.to_string());
    }
    None
}

/// First string-literal argument of a call like `IconPair::new("glyph", _)`.
fn icon_pair_first_str(expr: &syn::Expr) -> Option<String> {
    if let syn::Expr::Call(c) = expr
        && let Some(syn::Expr::Lit(l)) = c.args.first()
        && let syn::Lit::Str(s) = &l.lit
    {
        return Some(s.value());
    }
    None
}

/// Summarize the mascot from frames.rs source: counts `*_FRAMES` static tables
/// for the state count. Name and description are static brand facts. Returns
/// `None` when the source is missing, unparseable, or has no `*_FRAMES` table.
pub fn mascot_summary(src: &str) -> Option<Mascot> {
    let file = syn::parse_file(src).ok()?;
    let frame_count = file.items.iter().filter(|it| is_frames_static(it)).count();
    if frame_count == 0 {
        return None;
    }
    Some(Mascot {
        name: "Clawd".to_string(),
        frame_count: frame_count as u32,
        description: format!("ASCII-block mascot with {frame_count} animation states"),
    })
}

/// True for a `static *_FRAMES: ... = ...;` item.
fn is_frames_static(item: &syn::Item) -> bool {
    if let syn::Item::Static(s) = item {
        return s.ident.to_string().ends_with("_FRAMES");
    }
    false
}

/// Orchestrate the three extractors into a [`DesignSystem`]. Takes already-read
/// source strings (caller does the file I/O, matching `collect_features`). Empty
/// strings yield empty sections. `styles`/`layout_conventions` stay empty.
pub fn collect(theme_src: &str, icons_src: &str, frames_src: &str) -> DesignSystem {
    DesignSystem {
        palette: palette_from_source(theme_src),
        styles: Vec::new(),
        icons: icons_from_source(icons_src),
        mascot: mascot_summary(frames_src),
        layout_conventions: Vec::new(),
    }
}
