fn main() {
    // Embed all templates from the templates/ directory
    minijinja_embed::embed_templates!("templates");
}
