use cosmic_foundry::model::{compute_bbox, Anchor, Contour, GlyphOutline};
use cosmic_foundry::vecglypher::{
    build_ollama_chat_payload, build_openai_chat_payload, decode_vecglypher_svg,
    format_user_prompt, render_glyph_exemplar_base64, render_glyph_exemplar_png,
    VecGlypherPromptConfig, VECGLYPHER_RECOMMENDED_GGUF, VECGLYPHER_SYSTEM_PROMPT,
};
use image::GenericImageView;

#[test]
fn test_prompt_builders_text_and_multimodal() {
    assert!(!VECGLYPHER_RECOMMENDED_GGUF.is_empty());

    let default_cfg = VecGlypherPromptConfig::default();
    assert_eq!(default_cfg.target_text, "A");
    assert_eq!(default_cfg.temperature, 0.0);
    assert_eq!(default_cfg.top_k, 20);
    assert_eq!(default_cfg.max_tokens, 1024);

    let formatted = format_user_prompt("  geometric bold  ", "B");
    assert_eq!(
        formatted,
        "Font design requirements: geometric bold\nText content: B"
    );

    let fallback = format_user_prompt("   \t\n ", "C");
    assert_eq!(
        fallback,
        "Font design requirements: clean modern sans-serif, 400 weight, normal style\nText content: C"
    );

    // Ollama text-only payload
    let ollama_text = build_ollama_chat_payload("vecglypher:latest", &default_cfg, &[]);
    assert_eq!(ollama_text["model"], "vecglypher:latest");
    assert_eq!(ollama_text["stream"], false);
    assert_eq!(
        ollama_text["messages"][0]["content"],
        VECGLYPHER_SYSTEM_PROMPT
    );
    assert_eq!(ollama_text["messages"][1]["role"], "user");
    assert!(ollama_text["messages"][1].get("images").is_none());
    assert_eq!(ollama_text["options"]["num_predict"], 1024);
    assert_eq!(ollama_text["options"]["top_k"], 20);

    // Ollama multimodal payload
    let imgs = vec!["AAAA".to_string(), "BBBB".to_string()];
    let ollama_mm = build_ollama_chat_payload("vecglypher:latest", &default_cfg, &imgs);
    let images_arr = ollama_mm["messages"][1]["images"]
        .as_array()
        .expect("images array present");
    assert_eq!(images_arr.len(), 2);
    assert_eq!(images_arr[0], "AAAA");
    assert_eq!(images_arr[1], "BBBB");

    // OpenAI text-only payload
    let openai_text = build_openai_chat_payload("VecGlypher-27b", &default_cfg, &[]);
    assert_eq!(openai_text["model"], "VecGlypher-27b");
    assert_eq!(openai_text["stream"], false);
    assert_eq!(
        openai_text["chat_template_kwargs"]["enable_thinking"],
        false
    );
    assert!(openai_text["messages"][1]["content"].is_string());

    // OpenAI multimodal payload
    let openai_mm = build_openai_chat_payload("VecGlypher-27b", &default_cfg, &imgs);
    let content_arr = openai_mm["messages"][1]["content"]
        .as_array()
        .expect("multimodal content array");
    assert_eq!(content_arr.len(), 3);
    assert_eq!(content_arr[0]["type"], "image_url");
    assert_eq!(
        content_arr[0]["image_url"]["url"],
        "data:image/png;base64,AAAA"
    );
    assert_eq!(
        content_arr[1]["image_url"]["url"],
        "data:image/png;base64,BBBB"
    );
    assert_eq!(content_arr[2]["type"], "text");
    assert!(content_arr[2]["text"]
        .as_str()
        .expect("text content")
        .contains("Text content: A"));
}

#[test]
fn test_decode_vecglypher_svg_clean_transform_think_and_truncated() {
    // 1. Clean triangle <path>
    let clean = decode_vecglypher_svg(r#"<path d="M 10 80 L 50 10 L 90 80 Z"/>"#, 1000)
        .expect("clean svg path");
    assert_eq!(clean.len(), 1);
    assert!(clean[0].closed);
    let bbox = compute_bbox(&clean);
    assert!((bbox[3] - bbox[1] - 720.0).abs() < 2.0);

    // 2. Multi-path with transform="translate(10 20) scale(1.5 1.5)"
    let multi_svg = concat!(
        r#"<path d="M 0 0 L 10 0 L 10 10 L 0 10 Z" transform="translate(10 20) scale(1.5 1.5)"/>"#,
        "\n",
        r#"<path d="M 0 0 L 10 0 L 10 10 L 0 10 Z"/>"#
    );
    let multi = decode_vecglypher_svg(multi_svg, 1000).expect("multi-path with transform");
    assert_eq!(multi.len(), 2);
    let b0 = compute_bbox(&multi[0..1]);
    let b1 = compute_bbox(&multi[1..2]);
    let (big, small) = if (b0[2] - b0[0]) > (b1[2] - b1[0]) {
        (b0, b1)
    } else {
        (b1, b0)
    };
    let ratio = (big[2] - big[0]) / (small[2] - small[0]);
    assert!(
        (ratio - 1.5).abs() < 1e-2,
        "expected 1.5x scale ratio, got {ratio}"
    );

    // 3. <think>...</think> prefix + markdown fences
    let with_think = concat!(
        "<think>\nLet me plan a triangle.\n<path d=\"M 0 0 L 1 1 L 2 0 Z\"/>\n</think>\n",
        "```svg\n",
        "<svg viewBox=\"0 0 100 100\">\n",
        "  <path d=\"M 10 80 L 50 10 L 90 80 Z\"/>\n",
        "</svg>\n",
        "```\n"
    );
    let stripped = decode_vecglypher_svg(with_think, 1000).expect("think + fence decode");
    assert_eq!(stripped.len(), 1);
    assert_eq!(stripped[0].anchors.len(), 3);

    // 4. Truncated unclosed <path d="M 0 0 L 100 0 L 100 100 L 0 100 L 50
    let truncated = r#"<path d="M 0 0 L 100 0 L 100 100 L 0 100 L 50"#;
    let recovered = decode_vecglypher_svg(truncated, 1000).expect("truncated recovery");
    assert!(!recovered.is_empty());
    assert!(recovered[0].anchors.len() >= 4);

    // 5. Raw path string without <path> tag
    let raw_d = decode_vecglypher_svg("M 10 80 L 50 10 L 90 80 Z", 1000).expect("raw d");
    assert_eq!(raw_d.len(), 1);

    // 6. Invalid / empty input fails cleanly
    assert!(decode_vecglypher_svg("   ", 1000).is_err());
    assert!(decode_vecglypher_svg("not an svg output", 1000).is_err());
}

#[test]
fn test_render_glyph_exemplar_png_and_base64() {
    // Hollow square glyph ('O'-like): CW outer, CCW inner hole in Y-up font coordinates.
    let outer = Contour {
        anchors: vec![
            Anchor::on_curve(100.0, 0.0),
            Anchor::on_curve(100.0, 700.0),
            Anchor::on_curve(600.0, 700.0),
            Anchor::on_curve(600.0, 0.0),
        ],
        closed: true,
    };
    let inner = Contour {
        anchors: vec![
            Anchor::on_curve(250.0, 150.0),
            Anchor::on_curve(450.0, 150.0),
            Anchor::on_curve(450.0, 550.0),
            Anchor::on_curve(250.0, 550.0),
        ],
        closed: true,
    };
    let glyph = GlyphOutline {
        codepoint: 'O' as u32,
        name: "O".to_string(),
        advance_width: 700,
        lsb: 100,
        contours: vec![outer, inner],
    };

    let png_bytes = render_glyph_exemplar_png(&glyph, 1000, 192).expect("render png");
    let img = image::load_from_memory(&png_bytes).expect("decode png");
    assert_eq!(img.dimensions(), (192, 192));

    let gray = img.to_luma8();
    // Top-left corner in 14% margin must be white background (255).
    assert_eq!(gray.get_pixel(2, 2).0[0], 255);
    // Center of the hollow counter must also be white background (255).
    assert_eq!(gray.get_pixel(96, 96).0[0], 255);

    let mut has_white = false;
    let mut has_black = false;
    for p in gray.pixels() {
        if p.0[0] == 255 {
            has_white = true;
        } else if p.0[0] == 0 {
            has_black = true;
        }
    }
    assert!(has_white, "exemplar PNG must contain white background");
    assert!(has_black, "exemplar PNG must contain black foreground ink");

    // Base64 verification (PNG header base64 prefix)
    let b64 = render_glyph_exemplar_base64(&glyph, 1000, 192).expect("render base64");
    assert!(b64.starts_with("iVBORw0KGgo"));
    assert!(b64
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '='));

    // Size clamping & empty glyph fallback
    let empty_glyph = GlyphOutline {
        codepoint: ' ' as u32,
        name: "space".to_string(),
        advance_width: 300,
        lsb: 0,
        contours: Vec::new(),
    };
    let clamped_small = render_glyph_exemplar_png(&empty_glyph, 1000, 8).expect("clamped 32");
    let decoded_small = image::load_from_memory(&clamped_small).expect("decode 32x32");
    assert_eq!(decoded_small.dimensions(), (32, 32));
    assert!(decoded_small.to_luma8().pixels().all(|p| p.0[0] == 255));
}
