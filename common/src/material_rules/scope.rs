use serde::Deserialize;

#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub(super) struct CoordinateScopeDef {
    #[serde(default)]
    cols: Option<[i32; 2]>,
    #[serde(default)]
    rows: Option<[i32; 2]>,
    #[serde(default)]
    cell_cols: Option<[i32; 2]>,
    #[serde(default)]
    cell_rows: Option<[i32; 2]>,
    #[serde(default)]
    edge_cols: Option<[i32; 2]>,
    #[serde(default)]
    edge_rows: Option<[i32; 2]>,
}

impl CoordinateScopeDef {
    pub(super) fn cell_cols(self) -> Option<[i32; 2]> {
        self.cell_cols.or(self.cols)
    }

    pub(super) fn cell_rows(self) -> Option<[i32; 2]> {
        self.cell_rows.or(self.rows)
    }

    pub(super) fn edge_cols(self) -> Option<[i32; 2]> {
        self.edge_cols.or(self.cols)
    }

    pub(super) fn edge_rows(self) -> Option<[i32; 2]> {
        self.edge_rows.or(self.rows)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum WallRuleRelation {
    #[default]
    On,
    Touching,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum SurfaceScope {
    Cell,
    Edge(WallRuleRelation),
}
