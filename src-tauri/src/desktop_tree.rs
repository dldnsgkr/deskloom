#![allow(dead_code)]
//! Interprets a snapshot of the Windows desktop window tree. Pure; compiles on every OS.

#[derive(Clone, Debug)]
pub struct WinNode {
    pub hwnd: isize,
    pub class: String,
    /// 0 means top-level.
    pub parent: isize,
    /// Order among siblings, smaller is higher in z-order.
    pub z: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DesktopHost {
    /// 23H2 and older: the top-level WorkerW right behind the top-level window holding SHELLDLL_DefView.
    Legacy { workerw: isize, defview_parent: isize },
    /// 24H2 and newer (assumed structure, verify in Task 8): Progman holds both SHELLDLL_DefView and WorkerW.
    Progman { progman: isize, defview: isize, workerw: isize },
}

/// The snapshot as one line for the diagnostics: `class hwnd parent=… z=…` per node, in order.
pub fn describe(nodes: &[WinNode]) -> String {
    nodes.iter()
        .map(|n| format!("{} {:#x} parent={:#x} z={}", n.class, n.hwnd, n.parent, n.z))
        .collect::<Vec<_>>()
        .join("; ")
}

pub fn choose_desktop_host(nodes: &[WinNode]) -> Option<DesktopHost> {
    const DEFVIEW: &str = "SHELLDLL_DefView";
    let defview = nodes.iter().find(|n| n.class == DEFVIEW)?;
    let p = nodes.iter().find(|n| n.hwnd == defview.parent)?;

    // 24H2+: the parent is Progman and it also has a direct WorkerW child.
    if p.class == "Progman" {
        if let Some(w) = nodes.iter().find(|n| n.class == "WorkerW" && n.parent == p.hwnd) {
            return Some(DesktopHost::Progman { progman: p.hwnd, defview: defview.hwnd, workerw: w.hwnd });
        }
    }
    // Progman without a WorkerW child: not ready yet, the caller re-sends 0x052C and retries.
    if p.class == "Progman" {
        return None;
    }
    // 23H2-: DefView moved into a top-level non-Progman window; take the top-level WorkerW just behind it.
    if p.parent == 0 {
        return nodes
            .iter()
            .filter(|n| n.class == "WorkerW" && n.parent == 0 && n.z > p.z)
            .min_by_key(|n| n.z)
            .map(|w| DesktopHost::Legacy { workerw: w.hwnd, defview_parent: p.hwnd });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(hwnd: isize, class: &str, parent: isize, z: u32) -> WinNode { WinNode { hwnd, class: class.into(), parent, z } }

    #[test] fn legacy_picks_the_workerw_right_after_the_one_holding_defview() {
        let nodes = [n(10,"WorkerW",0,3), n(11,"SHELLDLL_DefView",10,0), n(12,"WorkerW",0,4), n(13,"WorkerW",0,7), n(1,"Progman",0,5)];
        assert_eq!(choose_desktop_host(&nodes), Some(DesktopHost::Legacy { workerw: 12, defview_parent: 10 }));
    }
    #[test] fn win11_24h2_uses_progman_children() {
        let nodes = [n(1,"Progman",0,5), n(11,"SHELLDLL_DefView",1,0), n(12,"WorkerW",1,1)];
        assert_eq!(choose_desktop_host(&nodes), Some(DesktopHost::Progman { progman: 1, defview: 11, workerw: 12 }));
    }
    #[test] fn progman_without_workerw_child_ignores_stray_toplevel_workerw() {
        let nodes = [n(1,"Progman",0,5), n(11,"SHELLDLL_DefView",1,0), n(20,"WorkerW",0,6)];
        assert_eq!(choose_desktop_host(&nodes), None);
    }
    #[test] fn describe_lists_class_hwnd_parent_and_z() {
        let nodes = [n(0x10,"Progman",0,5), n(0x11,"SHELLDLL_DefView",0x10,0)];
        assert_eq!(describe(&nodes), "Progman 0x10 parent=0x0 z=5; SHELLDLL_DefView 0x11 parent=0x10 z=0");
        assert_eq!(describe(&[]), "");
    }
    #[test] fn none_before_progman_spawns_workerw() {
        let nodes = [n(1,"Progman",0,5), n(11,"SHELLDLL_DefView",1,0)];
        assert_eq!(choose_desktop_host(&nodes), None);
    }
}
