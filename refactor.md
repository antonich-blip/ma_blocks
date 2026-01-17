1.  Refactor main.rs:
    *   Extract the CentralPanel and TopBottomPanel rendering into separate methods (e.g., render_canvas, render_toolbar).
    *   Move block-specific UI logic (like render_block and handle_resizing) into block.rs or a new ui module.
2.  Simplify Types: Create type aliases for complex generics like the image loading channel results.
3.  Group Parameters: Introduce a BlockRenderConfig or similar struct to pass UI state to rendering functions instead of individual booleans.
4.  Modernize & Clean Up:
    *   Inline format arguments as suggested by Clippy (e.g., format!("{err}")).
    *   Collapse nested if statements to reduce cognitive load.
5.  Add Documentation: Add doc comments explaining the purpose of core structs and the "why" behind key constants.


*   Simplifying complex types (like the image loading channel).
*   Refactoring render_block to reduce its parameter count.
*   Adding documentation to core structs and methods.
*   Moving UI-related logic from main.rs to a dedicated ui module.


 After this wait for user test and user confirmation on a next step.
