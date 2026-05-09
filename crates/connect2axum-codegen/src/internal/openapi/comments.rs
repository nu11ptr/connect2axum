use crate::internal::ir::CommentSet;

pub fn comment_description(comments: &CommentSet) -> Option<String> {
    let lines = comments
        .leading_detached
        .iter()
        .chain(comments.leading.iter())
        .flat_map(|comment| comment.lines())
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();

    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}
