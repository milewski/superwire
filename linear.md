# Linear Task Management Instructions

When working with Linear tasks, follow this workflow:

1. **Fetch Tasks**: Call the Linear MCP to fetch tasks assigned to me on the AI Engine project
2. **Start Work**: For any task in "Todo" status, begin working immediately
3. **Implementation**: 
   - Ensure all implementations are complete with no leftover or incomplete code
   - Delete all debugging code and temporary files during the process
   - Run all tests and ensure they pass before committing
4. **Code Quality**: Run formatting and linting:
   ```bash
   cargo clippy --fix --allow-dirty
   cargo fmt
   ```
5. **Commit**: Use git to commit changes with a descriptive message
6. **Pull Request**: Create a pull request for the changes
7. **Update Linear**: Mark the task as completed in Linear after the PR is created

## Important Checks Before Committing

- All tests passing
- No debugging code or console logs
- No temporary files
- Code formatted and linted
- Implementation is complete and functional
