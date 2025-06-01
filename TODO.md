# TODO - Tool Enhancement Ideas

## read.rs Enhancements
- [ ] Add `encoding` parameter to handle different text encodings (UTF-8, UTF-16, etc.)
- [ ] Add `tail` mode to read from end of file (useful for log files)
- [ ] Add pattern matching to return only lines containing specific text

## write.rs Enhancements
- [ ] Add `append` mode instead of always overwriting
- [ ] Add `encoding` parameter for different text encodings
- [ ] Add `backup` option to create a backup before overwriting

## edit.rs Enhancements
- [ ] Add regex support for pattern matching (currently only exact string match)
- [ ] Add `case_insensitive` option for replacements
- [ ] Add line number range support (e.g., only edit lines 10-50)

## list.rs Enhancements
- [ ] Add `recursive` option to list subdirectories
- [ ] Add `filter` parameter for glob patterns (*.js, *.py, etc.)
- [ ] Add `sort_by` option (name, size, modified date)
- [ ] Add `show_hidden` option for dotfiles
- [ ] Return file metadata (size, permissions, modified time)

## move_file.rs & copy.rs Enhancements
- [ ] Add `preserve_metadata` option (timestamps, permissions)
- [ ] Add `recursive` option for directories
- [ ] Add progress callback for large operations

## delete.rs Enhancements
- [ ] Add `recursive` option for directories (currently seems to only handle files)
- [ ] Add `force` option to skip confirmation
- [ ] Add pattern matching for bulk deletes

## touch.rs Enhancements
- [ ] Add ability to set specific timestamps instead of just "now"
- [ ] Add `reference` parameter to copy timestamps from another file

## chmod.rs Enhancements
- [ ] Add symbolic mode support (u+x, g-w) in addition to octal
- [ ] Add `preserve_special` for setuid/setgid bits

## Global Improvements
- [ ] Add `dry_run` mode to preview operations without executing
- [ ] Add transaction support to rollback on failure
- [ ] Add glob pattern support consistently across all tools
- [ ] Add progress reporting for long operations

## Priority Enhancements
The most impactful additions would be:
1. **Recursive operations** for copy, move, delete
2. **Pattern matching** for list, delete, edit operations  
3. **Append mode** for write operations
4. **File metadata** in list results