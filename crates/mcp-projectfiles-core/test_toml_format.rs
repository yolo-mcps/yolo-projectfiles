fn main() {
    let output = r#"
result = [1, 2, 3]
"#;
    
    if let Some(start) = output.find("result = ") {
        let extracted = &output[start + 9..];
        println\!("Extracted: '{}'", extracted);
    }
}
