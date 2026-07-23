#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum WindowKind {
    TreeView,
    ListView,
    Header,
    Other,
}

pub fn classify_window(class_name: &str) -> WindowKind {
    if class_name == "SysTreeView32" {
        WindowKind::TreeView
    } else if class_name == "SysListView32" {
        WindowKind::ListView
    } else if class_name == "SysHeader32" {
        WindowKind::Header
    } else {
        WindowKind::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_treeview() {
        assert_eq!(classify_window("SysTreeView32"), WindowKind::TreeView);
    }

    #[test]
    fn classifies_listview() {
        assert_eq!(classify_window("SysListView32"), WindowKind::ListView);
    }

    #[test]
    fn classifies_header() {
        assert_eq!(classify_window("SysHeader32"), WindowKind::Header);
    }

    #[test]
    fn classifies_unknown_as_other() {
        assert_eq!(classify_window("Button"), WindowKind::Other);
        assert_eq!(classify_window(""), WindowKind::Other);
        assert_eq!(classify_window("SomeMFCWndClassW"), WindowKind::Other);
    }
}
