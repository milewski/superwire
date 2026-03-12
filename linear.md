# Linear Task Management Instructions

When working with Linear tasks, follow this workflow:

1. **Fetch Tasks**: Call the Linear MCP to fetch tasks assigned to me on the AI Engine project
2. **Assess Complexity**: Evaluate if the task is large or complex
   - If the task involves multiple distinct features, components, or would take significant implementation effort, break it down
   - Create sub-issues in Linear for each logical piece of work
   - Assign the sub-issues to yourself to be worked on sequentially
   - Work on sub-issues one by one in order
3. **Start Work**: For any task in "Todo" status (that isn't too complex), begin working immediately
4. **Implementation**: 
   - Ensure all implementations are complete with no leftover or incomplete code
   - Delete all debugging code and temporary files during the process
   - Run all tests and ensure they pass before committing
5. **Code Quality**: Run formatting and linting:
   ```bash
   cargo clippy --fix --allow-dirty
   cargo fmt
   ```
6. **Commit**: Use git to commit changes with a descriptive message
7. **Pull Request**: Create a pull request for the changes
8. **Update Linear**: Mark the task as completed in Linear after the PR is created

## Important Checks Before Committing

- All tests passing
- No debugging code or console logs
- No temporary files
- Code formatted and linted
- Implementation is complete and functional

## Breaking Down Complex Tasks

When a task is too big or complex:
- Create sub-issues that represent logical, independent pieces of work
- Each sub-issue should be completable in a single focused session
- Assign all sub-issues to yourself
- Work through them sequentially, one at a time
- Only mark the parent task as complete when all sub-issues are done
