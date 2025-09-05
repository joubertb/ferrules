#[cfg(test)]
mod tests {
    use ferrules_core::debug::get_debug_file_path;

    #[test]
    fn test_path_traversal_protection() {
        // Valid names should work
        assert!(get_debug_file_path("valid-doc-name").is_ok());
        assert!(get_debug_file_path("normal123").is_ok());
        assert!(get_debug_file_path("uuid-like-name").is_ok());

        // Path traversal attempts should be blocked
        assert!(get_debug_file_path("../../../etc/passwd").is_err());
        assert!(get_debug_file_path("../../file").is_err());
        assert!(get_debug_file_path("../file").is_err());

        // Absolute paths should be blocked
        assert!(get_debug_file_path("/absolute/path").is_err());

        // Path separators should be blocked
        assert!(get_debug_file_path("with/slash").is_err());
        assert!(get_debug_file_path("with\\backslash").is_err());

        // Hidden files should be blocked
        assert!(get_debug_file_path(".hidden").is_err());

        // Empty names should be blocked
        assert!(get_debug_file_path("").is_err());

        // Very long names should be blocked
        let long_name = "a".repeat(300);
        assert!(get_debug_file_path(&long_name).is_err());
    }

    #[test]
    fn test_error_messages() {
        // Check that the error messages are correct
        let result = get_debug_file_path("../../../etc/passwd");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Path traversal not allowed"));

        let result = get_debug_file_path("with/slash");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Path separators not allowed"));

        let result = get_debug_file_path(".hidden");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid document name format"));
    }
}
