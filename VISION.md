# Vision

**Cluaiz is not only a product. It is a mission:** one trustworthy system where anyone, a single person or a whole
company, can use the full power of AI for their own work, without setting up and maintaining ten different tools.

## What we are building

- **Your models, your choice.** Run a model on your own hardware when you have it. Connect a hosted model API when
  that suits you better. The Engine is built to make running models on your own hardware fast and simple, but
  Cluaiz is not limited to that: the goal is one place to manage every model you use, wherever it runs.
- **One system instead of many.** Chat, vision, speech, embeddings, documents, agents and tools, served to your
  own apps through standard APIs, for personal use and for business use.
- **The full power of AI.** Whatever is possible with AI should be possible through Cluaiz. What is missing today is
  a gap to close, not a limit.
- **Best performance on the hardware you have.** From a 4 GB laptop GPU to a server, NVIDIA, AMD, Intel or Apple:
  the Engine measures the machine and decides where a model runs, and you can override every choice.
- **Secure and trustworthy.** API tokens on every route, approvals before actions that change things, an audit log,
  and your keys and data kept where you put them.
- **Light and native.** One executable written in Rust, small GPU packs, fast start.

## Principles

- **Say what is true.** A feature is documented as working only after it has been run, and speed is claimed only
  with a benchmark anyone can repeat.
- **Nothing happens behind your back.** No telemetry. Nothing is downloaded or installed without your command.
- **Open standards first.** OpenAI-compatible APIs, MCP, Agent Skills, GGUF and ONNX, so what you build with Cluaiz
  is not locked to it.
- **Your feedback sets the order.** What users report missing is what we build next.

## Where we are

This is a **pre-release**. The base of the architecture is built and works, and it is now being tested on many
machines so it becomes stable quickly. Parts of it can still change while that happens. Contributions of code open
with the alpha release; issues, hardware reports and feedback are welcome now. See [CONTRIBUTING.md](CONTRIBUTING.md).

On the way:

- Connecting hosted model APIs, including a future Cluaiz API, next to models on your own hardware.
- Tested support on macOS (the Metal GPU in automatic placement), AMD and Intel GPUs.
- More tools in [cluaiz-tools](https://github.com/cluaiz/cluaiz-tools).

If something you need is missing, tell us in an [issue](https://github.com/cluaiz/cluaiz/issues) or on
[Discord](https://discord.gg/mab8kBURz). That is how it gets built.
