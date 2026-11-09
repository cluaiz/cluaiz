# Contributing to Cluaiz

Thank you for your interest. Cluaiz is in **pre-release**, and what helps most right now is people trying it on
their own machines and telling us what works, what breaks and what is missing.

## What we accept during the pre-release

| Contribution | Accepted now |
| :--- | :--- |
| Bug reports | **Yes**, [open an issue](https://github.com/cluaiz/cluaiz/issues/new/choose) |
| Hardware reports: it works, or fails, on your CPU, GPU and OS | **Yes**, especially macOS, AMD, Intel and low-RAM machines |
| Feedback, ideas, "this is missing" | **Yes** |
| Questions and discussion | **Yes**, in issues or on [Discord](https://discord.gg/mab8kBURz) |
| Pull requests with code | **Not yet** |

### Why no pull requests yet

The base of the Engine's architecture is built and stable, and this pre-release exists to test it on as many
machines as possible so it becomes stable quickly. While that happens, parts of it can still change, sometimes
widely, for a new platform or an optimization. Merging outside code now would mean asking contributors to redo their
work again and again.

Pull requests to the Engine open when the pre-release ends with the alpha release. That can take a month or two.
Until then, a pull request is closed with thanks, and an issue that describes the same change is just as useful: it
shows us what to build.

**Tools are open to contributions now.** Skills, plugins and MCP servers live in
[cluaiz-tools](https://github.com/cluaiz/cluaiz-tools), each tool stands on its own, and new tools and fixes are
welcome there. See its [CONTRIBUTING.md](https://github.com/cluaiz/cluaiz-tools/blob/main/CONTRIBUTING.md).

## A good report

**A bug:** the output of `cluaiz --version` and `cluaiz status`, the model, the command or request, what you expected
and what happened. Logs help; remove tokens, keys and personal paths first.

**A hardware report:** the same, plus prompt and generation speed (`cluaiz bench <model>`) and whether it worked.

**Feedback:** what you were trying to do and why it matters to you. "I wanted to do X and could not" is the most
useful kind.

Security problems are never reported in public. See [SECURITY.md](SECURITY.md).

## Stay in touch

- **[Discord](https://discord.gg/mab8kBURz):** join if you want to contribute once pull requests open, or to talk
  about Cluaiz now. It has channels for contributors.
- **GitHub:** watch the repository for releases, and star it if Cluaiz is useful to you.

## Code of conduct

Be respectful and helpful. See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
