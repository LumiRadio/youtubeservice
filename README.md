# youtubeservice

Microservice responsible for fetching and sending YouTube LiveChat messages for ByersPlusPlus.

## Contributing

If you want to contribute, please take note of the following commands for setting up and maintaining various things:

### Git

#### Update submodules from GitHub

This step is important, when new protobuf files have been pushed or existing ones have been updated.
Make sure these are up to date, else youtubeservice will not compile correctly.

`git submodule update --recursive --remote`

### Building

`cargo build`

### Running

`cargo run --bin youtubeservice-server`

### Database

For managing databases, youtubeservice is using `diesel` and `diesel_cli`. Please make sure to install `diesel_cli` using `cargo install diesel_cli`.

#### Create migrations

`diesel migration generate [migration name]`

#### Apply migrations

`diesel migration run`

#### Redoing a migration (can be used to test down.sql files)

`diesel migration redo`
