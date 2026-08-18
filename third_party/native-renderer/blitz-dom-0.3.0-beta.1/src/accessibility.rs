use crate::{BaseDocument, Node as BlitzDomNode, local_name};
use accesskit::{Action, Node as AccessKitNode, NodeId, Role, Toggled, Tree, TreeId, TreeUpdate};

fn aria_bool(value: Option<&str>) -> bool {
    value.is_some_and(|value| value.eq_ignore_ascii_case("true"))
}

fn element_role(tag: &str, explicit: Option<&str>, input_type: Option<&str>) -> Role {
    if let Some(role) = explicit {
        return match role {
            "alert" => Role::Alert,
            "alertdialog" => Role::AlertDialog,
            "button" => Role::Button,
            "checkbox" => Role::CheckBox,
            "columnheader" => Role::ColumnHeader,
            "dialog" => Role::Dialog,
            "grid" => Role::Grid,
            "gridcell" => Role::GridCell,
            "link" => Role::Link,
            "listbox" => Role::ListBox,
            "menu" => Role::Menu,
            "menuitem" => Role::MenuItem,
            "navigation" => Role::Navigation,
            "option" => Role::ListBoxOption,
            "radio" => Role::RadioButton,
            "row" => Role::Row,
            "rowheader" => Role::RowHeader,
            "switch" => Role::Switch,
            "table" => Role::Table,
            "textbox" => Role::TextInput,
            _ => Role::Unknown,
        };
    }

    match tag {
        "a" => Role::Link,
        "button" => Role::Button,
        "dialog" => Role::Dialog,
        "div" => Role::GenericContainer,
        "footer" => Role::Footer,
        "form" => Role::Form,
        "header" => Role::Header,
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => Role::Heading,
        "input" => match input_type.unwrap_or("text") {
            "checkbox" => Role::CheckBox,
            "email" => Role::EmailInput,
            "number" => Role::NumberInput,
            "password" => Role::PasswordInput,
            "radio" => Role::RadioButton,
            "search" => Role::SearchInput,
            "tel" => Role::PhoneNumberInput,
            "url" => Role::UrlInput,
            _ => Role::TextInput,
        },
        "li" => Role::ListItem,
        "main" => Role::Main,
        "nav" => Role::Navigation,
        "ol" | "ul" => Role::List,
        "p" => Role::Paragraph,
        "section" => Role::Section,
        "select" => Role::ComboBox,
        "table" => Role::Table,
        "td" => Role::Cell,
        "textarea" => Role::MultilineTextInput,
        "th" => Role::ColumnHeader,
        "tr" => Role::Row,
        _ => Role::Unknown,
    }
}

impl BaseDocument {
    pub fn build_accessibility_tree(&self) -> TreeUpdate {
        let mut nodes = std::collections::HashMap::new();
        let mut window = AccessKitNode::new(Role::Window);

        self.visit(|node_id, node| {
            let parent = node
                .parent
                .and_then(|parent_id| nodes.get_mut(&parent_id))
                .map(|(_, parent)| parent)
                .unwrap_or(&mut window);
            let (id, builder) = self.build_accessibility_node(node, parent);

            nodes.insert(node_id, (id, builder));
        });

        let mut nodes: Vec<_> = nodes
            .into_iter()
            .map(|(_, (id, node))| (id, node))
            .collect();
        nodes.push((NodeId(u64::MAX), window));

        let tree = Tree::new(NodeId(u64::MAX));
        TreeUpdate {
            tree_id: TreeId::ROOT,
            nodes,
            tree: Some(tree),
            focus: NodeId(self.focus_node_id.map(|id| id as u64).unwrap_or(u64::MAX)),
        }
    }

    fn build_accessibility_node(
        &self,
        node: &BlitzDomNode,
        parent: &mut AccessKitNode,
    ) -> (NodeId, AccessKitNode) {
        let id = NodeId(node.id as u64);

        let mut builder = AccessKitNode::default();
        if node.id == 0 {
            builder.set_role(Role::Window)
        } else if let Some(element_data) = node.element_data() {
            let name = element_data.name.local.to_string();

            let role = element_role(
                &name,
                element_data.attr(local_name!("role")),
                element_data.attr(local_name!("type")),
            );

            builder.set_role(role);
            builder.set_html_tag(name);

            if let Some(label) = element_data
                .attr(local_name!("aria-label"))
                .or_else(|| element_data.attr(local_name!("title")))
            {
                builder.set_label(label);
            } else if matches!(role, Role::Button | Role::Link | Role::Heading) {
                let label = node.text_content();
                if !label.trim().is_empty() {
                    builder.set_label(label.trim());
                }
            }

            if let Some(description) = element_data.attr(local_name!("aria-description")) {
                builder.set_description(description);
            }
            if let Some(value) = element_data.attr(local_name!("value")) {
                builder.set_value(value);
            }

            if element_data.attr(local_name!("disabled")).is_some()
                || aria_bool(element_data.attr(local_name!("aria-disabled")))
            {
                builder.set_disabled();
            }
            if aria_bool(element_data.attr(local_name!("aria-selected"))) {
                builder.set_selected(true);
            }
            if let Some(expanded) = element_data.attr(local_name!("aria-expanded")) {
                builder.set_expanded(expanded.eq_ignore_ascii_case("true"));
            }
            if matches!(role, Role::CheckBox | Role::RadioButton | Role::Switch) {
                let checked = element_data.attr(local_name!("checked")).is_some()
                    || aria_bool(element_data.attr(local_name!("aria-checked")));
                builder.set_toggled(Toggled::from(checked));
            }
            if matches!(role, Role::Dialog | Role::AlertDialog)
                && aria_bool(element_data.attr(local_name!("aria-modal")))
            {
                builder.set_modal();
            }

            if node.is_focussable() {
                builder.add_action(Action::Focus);
                builder.add_action(Action::Blur);
            }
            if matches!(
                role,
                Role::Button
                    | Role::CheckBox
                    | Role::DefaultButton
                    | Role::DisclosureTriangle
                    | Role::Link
                    | Role::ListBoxOption
                    | Role::MenuItem
                    | Role::RadioButton
                    | Role::Switch
            ) {
                builder.add_action(Action::Click);
            }
            if let Some(expanded) = element_data.attr(local_name!("aria-expanded")) {
                builder.add_action(if expanded.eq_ignore_ascii_case("true") {
                    Action::Collapse
                } else {
                    Action::Expand
                });
            }
        } else if node.is_text_node() {
            builder.set_role(Role::TextRun);
            builder.set_value(node.text_content());
            parent.push_labelled_by(id)
        }

        parent.push_child(id);

        (id, builder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_native_semantic_roles() {
        assert_eq!(element_role("dialog", None, None), Role::Dialog);
        assert_eq!(element_role("input", None, Some("password")), Role::PasswordInput);
        assert_eq!(element_role("div", Some("gridcell"), None), Role::GridCell);
    }

    #[test]
    fn aria_boolean_is_strict_and_case_insensitive() {
        assert!(aria_bool(Some("TRUE")));
        assert!(!aria_bool(Some("1")));
        assert!(!aria_bool(None));
    }
}
