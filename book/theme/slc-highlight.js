// Highlight fenced ```sl blocks.
// mdBook 0.4.52 ships Highlight.js 10.1.1 and highlights in book.js before
// additional scripts run. `sl` is unknown on that first pass. This registers
// it and highlights the blocks. Highlight.js 10 exposes highlightBlock;
// highlightElement arrived in version 11.
(function () {
    if (typeof hljs === "undefined") {
        return;
    }

    hljs.registerLanguage("sl", function (hljs) {
        var KEYWORDS = [
            "func", "proc", "spec", "impl", "hook", "hand", "data", "enum",
            "menu", "form", "sect", "cite", "def", "pub", "for", "dual",
            "fn", "mu", "of", "do", "hn", "let", "reset"
        ].join(" ");
        return {
            name: "SLC",
            aliases: ["sl"],
            keywords: { keyword: KEYWORDS },
            contains: [
                hljs.C_LINE_COMMENT_MODE,
                hljs.C_BLOCK_COMMENT_MODE,
                {
                    className: "string",
                    begin: '"',
                    end: '"',
                    contains: [hljs.BACKSLASH_ESCAPE]
                },
                {
                    className: "string",
                    begin: "'",
                    end: "'",
                    contains: [hljs.BACKSLASH_ESCAPE]
                },
                {
                    className: "number",
                    begin: "\\b\\d[\\d_]*(\\.[\\d_]+)?\\b",
                    relevance: 0
                },
                {
                    className: "type",
                    begin: "\\b(i8|i32|i64|u8|u32|u64|f32|f64|String|Bool|File|Self)\\b"
                },
                {
                    className: "type",
                    begin: "\\b[A-Z][A-Za-z0-9_]*\\b",
                    relevance: 0
                }
            ]
        };
    });

    var highlight = typeof hljs.highlightBlock === "function"
        ? function (block) { hljs.highlightBlock(block); }
        : function (block) {
            block.classList.remove("hljs");
            block.removeAttribute("data-highlighted");
            hljs.highlightElement(block);
        };

    document.querySelectorAll("code.language-sl").forEach(highlight);
})();
