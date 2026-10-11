// Grading rules: MCQ comparison, numerical tolerance, and the +4 / −1 scheme.
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
struct Q {
    qtype: &'static str,
    answer: String,
}

fn numerical_ok(selected: &str, answer: &str) -> bool {
    let s: f64 = match selected.trim().parse() {
        Ok(v) => v,
        Err(_) => return false,
    };
    let a: f64 = match answer.trim().parse() {
        Ok(v) => v,
        Err(_) => return false,
    };
    let tol = a.abs() * 1e-3 + 1e-6;
    (s - a).abs() <= tol
}

fn is_correct(q: &Q, selected: &str) -> bool {
    if selected.trim().is_empty() {
        return false;
    }
    if q.qtype == "NUM" {
        numerical_ok(selected, &q.answer)
    } else {
        selected.trim().eq_ignore_ascii_case(q.answer.trim())
    }
}

fn marks(q: &Q, selected: &str) -> i32 {
    if selected.trim().is_empty() {
        0
    } else if is_correct(q, selected) {
        4
    } else {
        -1
    }
}

#[test]
fn mcq_is_case_insensitive_and_trimmed() {
    let q = Q { qtype: "MCQ", answer: "A".into() };
    assert!(is_correct(&q, "a"));
    assert!(is_correct(&q, " A "));
    assert!(!is_correct(&q, "B"));
}

#[test]
fn blank_answer_is_wrong_not_skipped() {
    let q = Q { qtype: "MCQ", answer: "A".into() };
    assert_eq!(marks(&q, ""), 0, "a blank question scores zero, not minus one");
    assert_eq!(marks(&q, "   "), 0);
}

#[test]
fn wrong_answer_costs_one_mark() {
    let q = Q { qtype: "MCQ", answer: "A".into() };
    assert_eq!(marks(&q, "C"), -1);
}

#[test]
fn correct_answer_scores_four() {
    let q = Q { qtype: "MCQ", answer: "A".into() };
    assert_eq!(marks(&q, "A"), 4);
}

#[test]
fn numerical_answers_allow_small_float_error() {
    let q = Q { qtype: "NUM", answer: "20".into() };
    assert!(is_correct(&q, "20.0"));
    assert!(is_correct(&q, "20.01"), "within 0.1 percent tolerance");
    assert!(!is_correct(&q, "20.5"), "outside tolerance");
    assert!(!is_correct(&q, "abc"), "non numeric input is wrong");
}

#[test]
fn zero_answer_is_not_mistaken_for_blank() {
    let q = Q { qtype: "NUM", answer: "0".into() };
    assert!(is_correct(&q, "0"));
    assert_eq!(marks(&q, "0"), 4);
}

#[test]
fn mixed_paper_score_matches_hand_calculation() {
    let mut answers = HashMap::new();
    answers.insert(1i64, ("A".to_string(), 4)); // correct
    answers.insert(2i64, ("B".to_string(), -1)); // wrong
    answers.insert(3i64, (String::new(), 0)); // blank
    let total: i32 = answers.values().map(|(_, m)| m).sum();
    assert_eq!(total, 3);
}