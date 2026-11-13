pub(crate) fn startup_failure_label(_error_detail: &str) -> &'static str {
    "TETHER_APP_STARTUP_FAILED"
}

#[cfg(test)]
mod tests {
    use super::startup_failure_label;

    #[test]
    fn startup_diagnostics_do_not_include_error_details() {
        let sensitive_detail = "draft-key-sentinel";
        let label = startup_failure_label(sensitive_detail);

        assert_eq!(label, "TETHER_APP_STARTUP_FAILED");
        assert!(!label.contains(sensitive_detail));
    }
}
