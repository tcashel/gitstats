# Contributing to GitStats

First off, thank you for considering contributing to GitStats! It's people like you that make GitStats such a great tool.

## Code of Conduct

This project and everyone participating in it is governed by our [Code of Conduct](CODE_OF_CONDUCT.md). By participating, you are expected to uphold this code.

## How Can I Contribute?

### Reporting Bugs

Before creating bug reports, please check the issue list as you might find out that you don't need to create one. When you are creating a bug report, please include as many details as possible:

* Use a clear and descriptive title
* Describe the exact steps which reproduce the problem
* Provide specific examples to demonstrate the steps
* Describe the behavior you observed after following the steps
* Explain which behavior you expected to see instead and why
* Include screenshots if possible

### Suggesting Enhancements

Enhancement suggestions are tracked as GitHub issues. Create an issue and provide the following information:

* Use a clear and descriptive title
* Provide a step-by-step description of the suggested enhancement
* Provide specific examples to demonstrate the steps
* Describe the current behavior and explain which behavior you expected to see instead
* Explain why this enhancement would be useful

### Pull Requests

* Fork the repo and create your branch from `main`
* If you've added code that should be tested, add tests
* Ensure the test suite passes
* Make sure your code follows the existing code style
* Write a good commit message

## Development Process

1. Fork the repository
2. Create a new branch: `git checkout -b my-branch-name`
3. Make your changes and commit them: `git commit -m 'Add some feature'`
4. Push to the branch: `git push origin my-branch-name`
5. Submit a pull request

### Development Setup

```bash
# Clone your fork
git clone https://github.com/your-username/gitstats.git

# Add the main repository as a remote
git remote add upstream https://github.com/original-owner/gitstats.git

# Install dependencies
cargo build
```

### Running Tests

```bash
cargo test
```

### Code Style

* We use `rustfmt` for code formatting
* Run `cargo fmt` before committing
* Use `cargo clippy` to catch common mistakes and improve your code

## Documentation

* Keep documentation up to date
* Document new features
* Update the README.md if needed

## Community

* Join our discussions in GitHub Discussions
* Follow our Twitter account
* Read our blog posts

## Questions?

Feel free to create an issue labeled as 'question' if you need help or clarification.

Thank you for contributing to GitStats! 🎉 