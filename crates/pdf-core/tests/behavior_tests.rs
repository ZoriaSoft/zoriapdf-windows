use pdf_core::collision::check_collision;
use pdf_core::range::parse_page_ranges;

#[test]
fn test_page_range_parsing_single_and_forward() {
    let pages = parse_page_ranges("1, 3-5, 8", 10).expect("Valid page range");
    assert_eq!(pages, vec![1, 3, 4, 5, 8]);
}

#[test]
fn test_page_range_parsing_reverse_for_reorder() {
    // Contract requirement: "Page ranges accept 1,3-5 and reverse 5-1 for reorder."
    let pages = parse_page_ranges("5-1", 5).expect("Valid reverse page range");
    assert_eq!(pages, vec![5, 4, 3, 2, 1]);

    let mixed = parse_page_ranges("4-2, 1", 4).expect("Valid mixed range");
    assert_eq!(pages_eq(mixed, vec![4, 3, 2, 1]), true);
}

fn pages_eq(a: Vec<u32>, b: Vec<u32>) -> bool {
    a == b
}

#[test]
fn test_page_range_parsing_empty_returns_all() {
    let pages = parse_page_ranges("", 4).expect("Empty range string should return all pages");
    assert_eq!(pages, vec![1, 2, 3, 4]);

    let whitespace = parse_page_ranges("   ", 3).expect("Whitespace should return all pages");
    assert_eq!(whitespace, vec![1, 2, 3]);
}

#[test]
fn test_page_range_parsing_out_of_bounds_errors() {
    assert!(parse_page_ranges("0", 5).is_err());
    assert!(parse_page_ranges("0-3", 5).is_err());
    assert!(parse_page_ranges("6", 5).is_err());
    assert!(parse_page_ranges("1-10", 5).is_err());
    assert!(parse_page_ranges("invalid", 5).is_err());
}

#[test]
fn test_collision_detection_refuses_input_overwrite() {
    // Contract requirement: "No overwriting original inputs; output collisions explicitly refused by engine."
    let inputs = vec!["document.pdf".to_string(), "notes.pdf".to_string()];

    // Same file should be refused
    assert!(check_collision(&inputs, "document.pdf", false).is_err());
    assert!(check_collision(&inputs, "notes.pdf", false).is_err());

    // Different file should be accepted
    assert!(check_collision(&inputs, "output.pdf", false).is_ok());
}

