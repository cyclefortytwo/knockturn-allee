# Knockturn Allee


## Install from the sources

1. Install Rust

2. Install Postgres

```
$ sudo apt update
$ sudo apt install postgresql postgresql-contrib
```

3. Set db password and create db

```
$ sudo -i -u postgres
>  \password <enter password>
> create database knockturn
```

4. Install diesel cli
```
$ sudo apt install gcc libpq-dev libssl-dev pkg-config
$ cargo install diesel_cli --no-default-features --features postgres
```

5. Install grin
Dwonload binary and run it.

6. Copy `env.sample` to `.env` and customize it:
- set pg password from step 3
- `WALLET_PASS` - content of `.api_secret` (eg file `~/.grin/floo/.api_secret`)
- `DOMAIN` - your hostname and port

7. Build the project
`cargo build`

8. Init db
`diesel migration run`

9. Run the project
