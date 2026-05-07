# Investigate SQL lite file format, port to Rust and improve performance.

* Clone sqlite into sqlite/ remove the .git dir.
* Research any exisiting Rust sqlite clones and wrappers to docs/research.md
* Analyse the file format to docs/file_format.md enough detail to write a reader/writer of the format.
* Analyse the SQL dialect used to docs/syntax.md
* Analyse the SQL planner to docs/planner.md
* Analyse the SQL engine to docs/engine.md
* Analyse the plugin mechanism to docs/plugins.md

When we have done the analysis, we will look at reproducing a popular sqlite wrapper.
