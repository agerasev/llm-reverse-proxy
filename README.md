# dashboard-assistant

## Build

```sh
docker build -t dashboard-assistant .
```

## Run

```sh
docker run \
    --name <container-name> \
    -e OPENAI_API_KEY=<key> \
    -p <port>:4000 \
    dashboard-assistant:latest
```

Simple chat client can be accessed by opening `http://localhost:<port>/` in your browser.

## Configure

Container can be configured using the following env vars:

+ `RUST_LOG` - log level, can be one of `error`, `warn`, `info`, `debug` or `trace`, default value is `info`.
+ `API_ADDR` - URL of remote API endpoint being proxied to, by default is `https://api.openai.com/`.
+ `API_KIND` - remote API kind, for now supported kinds are `openai` and `llama-cpp`, default kind is `openai`.
+ `OPENAI_API_KEY` - key to OpenAI API, used only with `openai` API kind.
+ `SYSTEM_PROMPT` - optional system prompt that will be prepended to user conversation, if set to `file:<path>` then loads prompt from file.

## Structure

### reverse-proxy

Service for proxying requests to LLMs via OpenAI API.
Can inject prompts and filter contents.
May be configured to multiplex requests to different servers.
Can serve static files for convenience.

### client-example

Simple web client used for demonstrational purpose.
