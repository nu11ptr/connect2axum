//! User confirmation for tools marked with
//! [`Tool::requires_confirmation`](crate::Tool::requires_confirmation).
//!
//! The server asks through MCP elicitation using the stateless multi
//! round-trip flow of protocol `2026-07-28`: the first `tools/call` returns
//! an `input_required` result holding a form elicitation, the client asks the
//! user, and then repeats the call with the user's answer in
//! `inputResponses`. The tool runs only when that answer is "accept".
//!
//! Older protocol revisions can only elicit over a session, which a
//! stateless server does not have, so a client that cannot take part gets an
//! error result instead of an unconfirmed call.

use std::collections::BTreeMap;

use connectrpc::ConnectError;
use rmcp::RoleServer;
use rmcp::model::{
    CallToolRequestParams, ElicitRequest, ElicitRequestParams, ElicitResult, ElicitationAction,
    ElicitationSchema, InputRequest, InputRequiredResult, JsonObject, ProtocolVersion,
};
use rmcp::service::RequestContext;

use crate::tool::{ToolResult, error_result};

/// The key of the confirmation in `inputRequests` and `inputResponses`.
const CONFIRM_KEY: &str = "confirm";
/// Longest argument text shown to the user before it is cut short.
const MAX_ARGUMENTS_SHOWN: usize = 2000;

pub(crate) enum Confirmation {
    /// The user accepted; run the tool.
    Granted,
    /// Ask the user, then expect the call again.
    Ask(InputRequiredResult),
    /// Do not run the tool; return this result.
    Refused(ToolResult),
}

pub(crate) fn confirm(
    request: &CallToolRequestParams,
    context: &RequestContext<RoleServer>,
) -> Confirmation {
    let name = request.name.as_ref();
    if !can_elicit(context) {
        return Confirmation::Refused(error_result(ConnectError::failed_precondition(format!(
            "{name} requires the user's confirmation, but this MCP client cannot ask for it; \
             it needs protocol version 2026-07-28 or later with form elicitation"
        ))));
    }

    let Some(response) = request
        .input_responses
        .as_ref()
        .and_then(|responses| responses.get(CONFIRM_KEY))
    else {
        return Confirmation::Ask(ask(name, request.arguments.as_ref()));
    };

    match serde_json::from_value::<ElicitResult>(response.clone()) {
        Ok(result) if result.action == ElicitationAction::Accept => Confirmation::Granted,
        Ok(_) => Confirmation::Refused(error_result(ConnectError::permission_denied(format!(
            "the user declined the call to {name}"
        )))),
        Err(err) => Confirmation::Refused(error_result(ConnectError::invalid_argument(format!(
            "invalid confirmation response for {name}: {err}"
        )))),
    }
}

/// Whether the request can carry an `input_required` round trip with a form
/// elicitation. A client declaring `elicitation: {}` supports forms.
fn can_elicit(context: &RequestContext<RoleServer>) -> bool {
    // ISO dates compare lexically the same as chronologically.
    let multi_round_trip = context
        .protocol_version()
        .is_some_and(|version| version.as_str() >= ProtocolVersion::V_2026_07_28.as_str());
    let form = context
        .client_capabilities()
        .and_then(|capabilities| capabilities.elicitation)
        .is_some_and(|elicitation| elicitation.form.is_some() || elicitation.url.is_none());
    multi_round_trip && form
}

fn ask(name: &str, arguments: Option<&JsonObject>) -> InputRequiredResult {
    let arguments = arguments.map_or_else(
        || "{}".to_owned(),
        |arguments| serde_json::to_string_pretty(arguments).unwrap_or_default(),
    );
    let message = format!(
        "Allow the tool {name} to run with these arguments?\n\n{}",
        truncate(&arguments, MAX_ARGUMENTS_SHOWN)
    );
    // No fields: the user's choice of accept, decline or cancel is the answer.
    let params = ElicitRequestParams::FormElicitationParams {
        meta: None,
        message,
        requested_schema: ElicitationSchema::new(BTreeMap::new()),
    };
    InputRequiredResult::from_input_requests(BTreeMap::from([(
        CONFIRM_KEY.to_owned(),
        InputRequest::Elicitation(ElicitRequest::new(params)),
    )]))
}

fn truncate(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((end, _)) => format!("{}\n…", &text[..end]),
        None => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::truncate;

    #[test]
    fn truncates_on_char_boundaries() {
        assert_eq!(truncate("abc", 3), "abc");
        assert_eq!(truncate("ééé", 2), "éé\n…");
    }
}
