// Highlight fenced ```sl blocks. mdBook highlights before additional scripts
// run, so this registers the language and highlights those blocks again.
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
                    begin: "-?\\b\\d[\\d_]*(\\.[\\d_]+)?\\b"
                },
                { className: "type", begin: "\\b(i8|i32|i64|u8|u32|u64|f32|f64|String|Bool|File|Self)\\b" }
            ]
        };
    });

    document.querySelectorAll("code.language-sl").forEach(function (block) {
        block.classList.remove("hljs");
        block.removeAttribute("data-highlighted");
        hljs.highlightElement(block);
    });
})();
