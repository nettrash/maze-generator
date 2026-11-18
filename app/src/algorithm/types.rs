pub enum AlgorithmType {
    Backtracking,
    Prim,
    Kruskal,
    AldousBroder,
    Wilson,
    Eller,
}

impl AlgorithmType {
    pub fn as_string(&self) -> &str {
        match self {
            AlgorithmType::Backtracking => "backtracking",
            AlgorithmType::Prim => "prim",
            AlgorithmType::Kruskal => "kruskal",
            AlgorithmType::AldousBroder => "aldous_broder",
            AlgorithmType::Wilson => "wilson",
            AlgorithmType::Eller => "eller",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AlgorithmType;

    #[test]
    fn as_string_returns_expected_values() {
        let cases = [
            (AlgorithmType::Backtracking, "backtracking"),
            (AlgorithmType::Prim, "prim"),
            (AlgorithmType::Kruskal, "kruskal"),
            (AlgorithmType::AldousBroder, "aldous_broder"),
            (AlgorithmType::Wilson, "wilson"),
            (AlgorithmType::Eller, "eller"),
        ];

        for (algorithm, expected) in cases {
            assert_eq!(algorithm.as_string(), expected);
        }
    }
}
