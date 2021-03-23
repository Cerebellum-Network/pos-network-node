# Cere Node with PoS consensus

## How to become a validator

Follow the [instructions](https://github.com/Cerebellum-Network/validator-instructions#how-to-become-a-validator) to get the Node up and running.

## License

- Cere Primitives (`sp-*`), Frame (`frame-*`) and the pallets (`pallets-*`), binaries (`/bin`) and all other utilities are licensed under [Apache 2.0](LICENSE-APACHE2).
- Cere Client (`/client/*` / `sc-*`) is licensed under [GPL v3.0 with a classpath linking exception](LICENSE-GPL3).

## Build the node

1. [Install `Docker`](https://docs.docker.com/get-docker/).
2. Run the following command to update submodules.

```
 git submodule update --init --recursive
```

3. Run the following command to build the node.

```
 docker build .
```

## Versioning strategy

The package must follow **Semantic Versioning** (SemVer).
This strategy provides information on the type of changes introduced in a given version, compared to the previous one, in a unified format that automated tools can use.

The version is expressed as **MAJOR.MINOR.PATCH**.

- MAJOR introduces one or more breaking changes.
- MINOR introduces one or more backwards-compatible API changes.
- PATCH only introduces bug fixes with no API changes.

| Increment this value                    | Under these conditions                                                                                                                                                                                                                                                                                                                                                    | Example                                                                                                                                                                                                                                                                            |
| :-------------------------------------- | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | :--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **MAJOR**                               | There is at least one breaking change in the current repository. Backwards-incompatible changes to the API surface (the exposed part of the API) or features in a way that risks compilation or runtime errors. A major version bump in the upstream repository upon syncing a fork. When incrementing the major version, always reset the PATCH and MINOR values to `0`. | Versions `2.1.0` and `3.0.0` are incompatible and cannot be used interchangeably without risk. Upstream version was incremented from `2.0.1` to `3.0.0` or higher.                                                                                                                 |
| **MINOR** (same **MAJOR** value)        | The highest MINOR introduces functionality in a backwards-compatible way, which includes: • Changing the API surface or features without risking compilation or runtime errors. • Adding non-API features. A minor version bump in the upstream repository upon syncing a fork. Note : When incrementing the minor version, always reset the PATCH version to `0`.        | It is possible to use `2.2.0` to satisfy a dependency on `2.1.0` because `2.2.0` is backwards-compatible. But it is not possible to use version `2.1.0` to satisfy a dependency on `2.2.0`. Upstream version was incremented from `2.0.1` to `2.1.0`, but not higher than `3.0.0`. |
| **PATCH** (same **MAJOR.MINOR** values) | The highest PATCH carries bug fixes without modifying the API at all. The API remains identical, and the features are not changed. A patch version bump in the upstream repository upon syncing a fork.                                                                                                                                                                   | Versions `2.2.0` and `2.2.1` should be interchangeable because they have the same API, even though `2.2.1` includes a bug fix not present in `2.2.0`.                                                                                                                              |
