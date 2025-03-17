# llm-reverse-proxy

## Build

```sh
docker build -t llm-reverse-proxy .
```

## Run

```sh
docker run \
    --name <container-name> \
    -e OPENAI_API_KEY=<key> \
    -p <port>:4000 \
    llm-reverse-proxy:latest
```

Simple chat client can be accessed by opening `http://localhost:<port>/` in your browser.

## Configure

Container can be configured using the following env vars:

+ `RUST_LOG` - log level, can be one of `error`, `warn`, `info`, `debug` or `trace`, default value is `info`.
+ `API_ADDR` - URL of remote API endpoint being proxied to, by default is `https://api.openai.com/`.
+ `OPENAI_API_KEY` - key to OpenAI API, used only with `api.openai.com` URL.
+ `SYSTEM_PROMPT` - optional system prompt that will be prepended to user conversation, if set to `file:<path>` then loads prompt from file.

## Structure

### reverse-proxy

Service for proxying requests to LLMs via OpenAI API.
Can inject prompts and filter contents.
May be configured to multiplex requests to different servers.
Can serve static files for convenience.

### client-example

Simple web client used for demonstrational purpose.
