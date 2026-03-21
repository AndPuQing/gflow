# Contributing to gflow

First off, thank you for considering contributing to gflow! It's people like you that make gflow such a great tool.

## Where do I go from here?

If you've noticed a bug or have a feature request, [make one](https://github.com/AndPuQing/gflow/issues/new)! It's generally best if you get confirmation of your bug or approval for your feature request this way before starting to code.

### Fork & create a branch

If this is something you think you can fix, then [fork gflow](https://github.com/AndPuQing/gflow/fork) and create a branch with a descriptive name.

A good branch name would be (where issue #33 is the ticket you're working on):

```sh
git checkout -b 33-add-new-command
```

### Get the code

```sh
git clone https://github.com/your-username/gflow.git
cd gflow
git checkout 33-add-new-command
```

### Setting up the development environment

gflow is built with Rust, so you'll need to have the Rust toolchain installed. You can find instructions on how to do that [here](https://www.rust-lang.org/tools/install).

Once you have Rust installed, you can build the project with:

```sh
cargo build
```

### Running the tests

You can run the tests with:

```sh
cargo test --workspace
```

### Submitting a pull request

When you're done with your changes, you can submit a pull request. Make sure to reference the issue you're working on in the pull request description.

### Code of Conduct

This project and everyone participating in it is governed by the [gflow Code of Conduct](CODE_OF_CONDUCT.md). By participating, you are expected to uphold this code. Please report unacceptable behavior to [me@puqing.work](mailto:me@puqing.work).
