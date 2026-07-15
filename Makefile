# BudBuk developer tasks. Run `make check` before pushing.
.PHONY: help build test fmt fmt-check lint doc cov cov-html cov-check deny run check

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}'

build: ## Build the whole workspace
	cargo build --workspace

test: ## Run all tests
	cargo test --workspace

fmt: ## Format the code
	cargo fmt --all

fmt-check: ## Check formatting (CI)
	cargo fmt --all -- --check

lint: ## Run clippy with warnings denied (CI)
	cargo clippy --workspace --all-targets -- -D warnings

doc: ## Build and open the API docs
	cargo doc --workspace --no-deps --open

cov: ## Print a coverage summary
	cargo llvm-cov --workspace --ignore-filename-regex 'main\.rs|examples/' --summary-only

cov-html: ## Generate an HTML coverage report
	cargo llvm-cov --workspace --ignore-filename-regex 'main\.rs|examples/' --html
	@echo "open target/llvm-cov/html/index.html"

cov-check: ## Fail if any line is uncovered (the 100%-line gate)
	cargo llvm-cov --workspace --ignore-filename-regex 'main\.rs|examples/' --lcov --output-path lcov.info
	@u=$$(awk -F, '/^DA:/ && $$2==0 {c++} END{print c+0}' lcov.info); \
		if [ "$$u" -ne 0 ]; then echo "FAIL: $$u uncovered line(s)"; exit 1; fi; \
		echo "OK: 100% line coverage"

deny: ## Run supply-chain / license checks
	cargo deny check

run: ## Run the Jira demo CLI
	cargo run -p jira-connector

check: fmt-check lint test cov-check ## Run everything CI runs
	@echo "All checks passed."
