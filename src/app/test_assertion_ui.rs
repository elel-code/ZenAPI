mod actions;
mod text;

pub(super) use self::actions::wire_test_assertion_actions;
pub(super) use self::text::test_assertion_table_model;
#[cfg(test)]
pub(super) use self::text::{
    add_custom_test_assertion_text, add_test_assertion_template_text, add_test_assertion_text,
    delete_test_assertion_text, next_test_assertion_template, test_assertion_template,
    update_test_assertion_text,
};
