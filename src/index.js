/// @ts-check

/** @return {never} */
function never() { throw new Error() }

/**@type{WebAssembly.Instance}*/ let hbcInstance;
/**@type{Promise<WebAssembly.WebAssemblyInstantiatedSource>}*/ let hbcInstaceFuture;
async function getHbcInstance() {
	hbcInstaceFuture ??= WebAssembly.instantiateStreaming(fetch("/hbc.wasm"), {});
	return hbcInstance ??= (await hbcInstaceFuture).instance;
}

const stack_pointer_offset = 1 << 20;

/** @param {WebAssembly.Instance} instance @param {Post[]} packages @param {number} fuel
 * @returns {string} */
function compileCode(instance, packages, fuel = 100) {
	let {
		INPUT, INPUT_LEN,
		LOG_MESSAGES, LOG_MESSAGES_LEN,
		memory, compile_and_run,
	} = instance.exports;

	if (!(true
		&& memory instanceof WebAssembly.Memory
		&& INPUT instanceof WebAssembly.Global
		&& INPUT_LEN instanceof WebAssembly.Global
		&& LOG_MESSAGES instanceof WebAssembly.Global
		&& LOG_MESSAGES_LEN instanceof WebAssembly.Global
		&& typeof compile_and_run === "function"
	)) never();

	const codeLength = packPosts(packages, new DataView(memory.buffer, INPUT.value));
	new DataView(memory.buffer).setUint32(INPUT_LEN.value, codeLength, true);

	runWasmFunction(instance, compile_and_run, fuel, packages.length);
	return bufToString(memory, LOG_MESSAGES, LOG_MESSAGES_LEN).trim();
}

/**@type{WebAssembly.Instance}*/ let fmtInstance;
/**@type{Promise<WebAssembly.WebAssemblyInstantiatedSource>}*/ let fmtInstaceFuture;
async function getFmtInstance() {
	fmtInstaceFuture ??= WebAssembly.instantiateStreaming(fetch("/hbfmt.wasm"), {});
	return fmtInstance ??= (await fmtInstaceFuture).instance;
}

/** @param {WebAssembly.Instance} instance @param {string} code @param {"tok" | "fmt" | "minify"} action
 * @returns {string | Uint8Array | undefined} */
function modifyCode(instance, code, action) {
	let {
		INPUT, INPUT_LEN,
		OUTPUT, OUTPUT_LEN,
		memory, fmt, tok, minify
	} = instance.exports;

	let funs = { fmt, tok, minify };
	let fun = funs[action];
	if (!(true
		&& memory instanceof WebAssembly.Memory
		&& INPUT instanceof WebAssembly.Global
		&& INPUT_LEN instanceof WebAssembly.Global
		&& OUTPUT instanceof WebAssembly.Global
		&& OUTPUT_LEN instanceof WebAssembly.Global
		&& funs.hasOwnProperty(action)
		&& typeof fun === "function"
	)) never();

	if (action !== "fmt") {
		INPUT = OUTPUT;
		INPUT_LEN = OUTPUT_LEN;
	}

	let dw = new DataView(memory.buffer);
	dw.setUint32(INPUT_LEN.value, code.length, true);
	new Uint8Array(memory.buffer, INPUT.value).set(new TextEncoder().encode(code));
	new Uint8Array(memory.buffer, INPUT.value + code.length)[0] = 0;

	if (!runWasmFunction(instance, fun)) {
		return undefined;
	}
	if (action === "tok") {
		return bufSlice(memory, OUTPUT, OUTPUT_LEN);
	} else {
		return bufToString(memory, OUTPUT, OUTPUT_LEN);
	}
}


/** @param {WebAssembly.Instance} instance @param {CallableFunction} func @param {any[]} args
 * @returns {boolean} */
function runWasmFunction(instance, func, ...args) {
	const { PANIC_MESSAGE, PANIC_MESSAGE_LEN, memory, __stack_pointer } = instance.exports;
	if (!(true
		&& memory instanceof WebAssembly.Memory
		&& __stack_pointer instanceof WebAssembly.Global
	)) never();
	const ptr = __stack_pointer.value;
	try {
		func(...args);
		return true;
	} catch (error) {
		if (error instanceof WebAssembly.RuntimeError
			&& error.message == "unreachable"
			&& PANIC_MESSAGE instanceof WebAssembly.Global
			&& PANIC_MESSAGE_LEN instanceof WebAssembly.Global) {
			console.error(bufToString(memory, PANIC_MESSAGE, PANIC_MESSAGE_LEN), error);
		} else {
			console.error(error);
		}
		__stack_pointer.value = ptr;
		return false;
	}
}

/** @typedef {Object} Post
 * @property {string} path 
 * @property {string} code */

/** @param {Post[]} posts @param {DataView} view  @returns {number} */
function packPosts(posts, view) {
	const enc = new TextEncoder(), buf = new Uint8Array(view.buffer, view.byteOffset);
	let len = 0; for (const post of posts) {
		view.setUint16(len, post.path.length, true); len += 2;
		buf.set(enc.encode(post.path), len); len += post.path.length;
		view.setUint16(len, post.code.length, true); len += 2;
		buf.set(enc.encode(post.code), len); len += post.code.length;
		buf[len] = 0; len += 1; // null teminate the code
	}
	return len;
}

/** @param {WebAssembly.Memory} mem
 * @param {WebAssembly.Global} ptr
 * @param {WebAssembly.Global} len
 * @return {Uint8Array} */
function bufSlice(mem, ptr, len) {
	return new Uint8Array(mem.buffer, ptr.value,
		new DataView(mem.buffer).getUint32(len.value, true));
}

/** @param {WebAssembly.Memory} mem
 * @param {WebAssembly.Global} ptr
 * @param {WebAssembly.Global} len
 * @return {string} */
function bufToString(mem, ptr, len) {
	const res = new TextDecoder()
		.decode(new Uint8Array(mem.buffer, ptr.value,
			new DataView(mem.buffer).getUint32(len.value, true)));
	new DataView(mem.buffer).setUint32(len.value, 0, true);
	return res;
}

/** @param {HTMLElement} target */
function wireUp(target) {
	execApply(target);
	cacheInputs(target);
	bindCodeEdit(target);
	bindTextareaAutoResize(target);
}

const importRe = /@use\s*\(\s*"(([^"]|\\")+)"\s*\)/g;

/** @param {WebAssembly.Instance} fmt 
 * @param {string} code
 * @param {string[]} roots
 * @param {Post[]} buf
 * @param {Set<string>} prevRoots
 * @returns {void} */
function loadCachedPackages(fmt, code, roots, buf, prevRoots) {
	buf[0].code = code;

	roots.length = 0;
	let changed = false;
	for (const match of code.matchAll(importRe)) {
		changed ||= !prevRoots.has(match[1]);
		roots.push(match[1]);
	}

	if (!changed) return;
	buf.length = 1;
	prevRoots.clear();

	for (let imp = roots.pop(); imp !== undefined; imp = roots.pop()) {
		if (prevRoots.has(imp)) continue; prevRoots.add(imp);

		const fmtd = modifyCode(fmt, localStorage.getItem("package-" + imp) ?? never(), "fmt");
		if (typeof fmtd != "string") never();
		buf.push({ path: imp, code: fmtd });
		for (const match of buf[buf.length - 1].code.matchAll(importRe)) {
			roots.push(match[1]);
		}
	}
}

/**@type{Set<string>}*/ const prevRoots = new Set();
/**@typedef {Object} PackageCtx
 * @property {AbortController} [cancelation]
 * @property {string[]} keyBuf
 * @property {Set<string>} prevParams
 * @property {HTMLTextAreaElement} [edit] */

/** @param {string} source @param {Set<string>} importDiff @param {HTMLPreElement} errors @param {PackageCtx} ctx */
async function fetchPackages(source, importDiff, errors, ctx) {
	importDiff.clear();
	for (const match of source.matchAll(importRe)) {
		if (localStorage["package-" + match[1]]) continue;
		importDiff.add(match[1]);
	}

	if (importDiff.size !== 0 && (ctx.prevParams.size != importDiff.size
		|| [...ctx.prevParams.keys()].every(e => importDiff.has(e)))) {
		if (ctx.cancelation) ctx.cancelation.abort();
		ctx.prevParams.clear();
		ctx.prevParams = new Set([...importDiff]);
		ctx.cancelation = new AbortController();

		ctx.keyBuf.length = 0;
		ctx.keyBuf.push(...importDiff.keys());

		errors.textContent = "fetching: " + ctx.keyBuf.join(", ");

		await fetch(`/code`, {
			method: "POST",
			signal: ctx.cancelation.signal,
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify(ctx.keyBuf),
		}).then(async e => {
			try {
				const json = await e.json();
				if (e.status == 200) {
					for (const [key, value] of Object.entries(json)) {
						localStorage["package-" + key] = value;
					}
					const missing = ctx.keyBuf.filter(i => json[i] === undefined);
					if (missing.length !== 0) {
						errors.textContent = "deps not found: " + missing.join(", ");
					} else {
						ctx.cancelation = undefined;
						ctx.edit?.dispatchEvent(new InputEvent("input"));
					}
				}
			} catch (er) {
				errors.textContent = "completely failed to fetch ("
					+ e.status + "): " + ctx.keyBuf.join(", ");
				console.error(e, er);
			}
		});
	}

}

/** @param {HTMLElement} target */
async function bindCodeEdit(target) {
	const edit = target.querySelector("#code-edit");
	if (!(edit instanceof HTMLTextAreaElement)) return;

	const codeSize = target.querySelector("#code-size");
	const errors = target.querySelector("#compiler-output");
	if (!(true
		&& codeSize instanceof HTMLSpanElement
		&& errors instanceof HTMLPreElement
	)) never();

	const MAX_CODE_SIZE = parseInt(codeSize.innerHTML);
	if (Number.isNaN(MAX_CODE_SIZE)) never();

	const hbc = await getHbcInstance(), fmt = await getFmtInstance();
	let importDiff = new Set();
	/**@type{Post[]}*/
	const packages = [{ path: "local.hb", code: "" }];
	const debounce = 100;
	let timeout = 0;
	const ctx = { keyBuf: [], prevParams: new Set(), edit };

	prevRoots.clear();

	const onInput = () => {
		fetchPackages(edit.value, importDiff, errors, ctx);

		if (ctx.cancelation && importDiff.size !== 0) {
			return;
		}

		loadCachedPackages(fmt, edit.value, ctx.keyBuf, packages, prevRoots);

		errors.textContent = compileCode(hbc, packages);
		const minified_size = modifyCode(fmt, edit.value, "minify")?.length;
		if (minified_size) {
			codeSize.textContent = (MAX_CODE_SIZE - minified_size) + "";
			const perc = Math.min(100, Math.floor(100 * (minified_size / MAX_CODE_SIZE)));
			codeSize.style.color = `color-mix(in srgb, light-dark(black, white), var(--error) ${perc}%)`;
		}
		timeout = 0;
	};

	edit.addEventListener("input", () => {
		if (timeout) clearTimeout(timeout);
		timeout = setTimeout(onInput, debounce)
	});
	edit.dispatchEvent(new InputEvent("input"));
}

/**
 * @type {Array<string>}
 * to be synched with `enum TokenGroup` in bytecode/src/fmt.rs */
const TOK_CLASSES = [
	'Blank',
	'Comment',
	'Keyword',
	'Identifier',
	'Directive',
	'Number',
	'String',
	'Op',
	'Assign',
	'Paren',
	'Bracket',
	'Colon',
	'Comma',
	'Dot',
	'Ctor',
];

/** @type {{ [key: string]: (el: HTMLElement) => void | Promise<void> }} */
const applyFns = {
	timestamp: (el) => {
		const timestamp = el.innerText;
		const date = new Date(parseInt(timestamp) * 1000);
		el.innerText = date.toLocaleString();
	},
	fmt,
};

/**
 * @param {HTMLElement} target */
async function fmt(target) {
	const code = target.innerText;
	const instance = await getFmtInstance();
	const decoder = new TextDecoder('utf-8');
	const fmt = modifyCode(instance, code, 'fmt');
	if (typeof fmt !== "string") return;
	const codeBytes = new TextEncoder().encode(fmt);
	const tok = modifyCode(instance, fmt, 'tok');
	if (!(tok instanceof Uint8Array)) return;
	target.innerHTML = '';
	let start = 0;
	let kind = tok[0];
	for (let ii = 1; ii <= tok.length; ii += 1) {
		// split over same tokens and buffer end
		if (tok[ii] === kind && ii < tok.length) {
			continue;
		}
		const text = decoder.decode(codeBytes.subarray(start, ii));
		const textNode = document.createTextNode(text);;
		if (kind === 0) {
			target.appendChild(textNode);
		} else {
			const el = document.createElement('span');
			el.classList.add('syn');
			el.classList.add(TOK_CLASSES[kind]);
			el.appendChild(textNode);
			target.appendChild(el);
		}
		if (ii == tok.length) {
			break;
		}
		start = ii;
		kind = tok[ii];
	}
}

/** @param {HTMLElement} target */
function execApply(target) {
	const proises = [];
	for (const elem of target.querySelectorAll('[apply]')) {
		if (!(elem instanceof HTMLElement)) continue;
		const funcname = elem.getAttribute('apply') ?? never();
		const vl = applyFns[funcname](elem);
		if (vl instanceof Promise) proises.push(vl);
	}
	if (target === document.body) {
		Promise.all(proises).then(() => document.body.hidden = false);
	}
}

/** @param {HTMLElement} target */
function bindTextareaAutoResize(target) {
	for (const textarea of target.querySelectorAll("textarea")) {
		if (!(textarea instanceof HTMLTextAreaElement)) never();

		const taCssMap = window.getComputedStyle(textarea);
		const padding = parseInt(taCssMap.getPropertyValue('padding-top') ?? "0")
			+ parseInt(taCssMap.getPropertyValue('padding-top') ?? "0");
		textarea.style.height = "auto";
		textarea.style.height = (textarea.scrollHeight - padding) + "px";
		textarea.style.overflowY = "hidden";
		textarea.addEventListener("input", function() {
			let top = window.scrollY;
			textarea.style.height = "auto";
			textarea.style.height = (textarea.scrollHeight - padding) + "px";
			window.scrollTo({ top });
		});

		textarea.onkeydown = (ev) => {
			if (ev.key === "Tab") {
				ev.preventDefault();
				document.execCommand('insertText', false, "\t");
			}
		}
	}
}

/** @param {HTMLElement} target */
function cacheInputs(target) {
	/**@type {HTMLFormElement}*/ let form;
	for (form of target.querySelectorAll('form')) {
		const path = form.getAttribute('hx-post') || form.getAttribute('hx-delete');
		if (!path) {
			console.warn('form does not have a hx-post or hx-delete attribute', form);
			continue;
		}

		for (const input of form.elements) {
			if (input instanceof HTMLInputElement || input instanceof HTMLTextAreaElement) {
				if ('password submit button'.includes(input.type)) continue;
				const key = path + input.name;
				input.value = localStorage.getItem(key) ?? '';
				input.addEventListener("input", () => localStorage.setItem(key, input.value));
			} else {
				console.warn("unhandled form element: ", input);
			}
		}
	}
}

/** @param {string} [path]  */
function updateTab(path) {
	console.log(path);
	for (const elem of document.querySelectorAll("button[hx-push-url]")) {
		if (elem instanceof HTMLButtonElement)
			elem.disabled =
				elem.getAttribute("hx-push-url") === path
				|| elem.getAttribute("hx-push-url") === window.location.pathname;
	}
}

if (window.location.hostname === 'localhost') {
	let id; setInterval(async () => {
		let new_id = await fetch('/hot-reload').then(reps => reps.text());
		id ??= new_id;
		if (id !== new_id) window.location.reload();
	}, 300);

	(async function test() {
		{
			const code = "main:=fn():void{return}";
			const inst = await getFmtInstance()
			const fmtd = modifyCode(inst, code, "fmt") ?? never();
			if (typeof fmtd !== "string") never();
			const prev = modifyCode(inst, fmtd, "minify") ?? never();
			if (code != prev) console.error(code, prev);
		}
		{
			const posts = [{
				path: "foo.hb",
				code: "main:=fn():int{return 42}",
			}];
			const res = compileCode(await getHbcInstance(), posts, 1) ?? never();
			const expected = "exit code: 42";
			if (expected != res) console.error(expected, res);
		}
	})()
}

document.body.addEventListener('htmx:afterSwap', (ev) => {
	if (!(ev.target instanceof HTMLElement)) never();
	wireUp(ev.target);
	if (ev.target.tagName == "MAIN" || ev.target.tagName == "BODY")
		updateTab(ev['detail'].pathInfo.finalRequestPath);
});

getFmtInstance().then(inst => {
	document.body.addEventListener('htmx:configRequest', (ev) => {
		const details = ev['detail'];
		if (details.path === "/post" && details.verb === "post") {
			details.parameters['code'] = modifyCode(inst, details.parameters['code'], "minify");
		}
	});

	/** @param {string} query @param {string} target @returns {number} */
	function fuzzyCost(query, target) {
		let qi = 0, bi = 0, cost = 0, matched = false;
		while (qi < query.length) {
			if (query.charAt(qi) === target.charAt(bi++)) {
				matched = true;
				qi++;
			} else {
				cost++;
			}
			if (bi === target.length) (bi = 0, qi++);
		}
		return cost + (matched ? 0 : 100 * target.length);
	}

	let deps = undefined;
	/** @param {HTMLInputElement} input @returns {void} */
	function filterCodeDeps(input) {
		deps ??= document.getElementById("deps");
		if (!(deps instanceof HTMLElement)) never();
		if (input.value === "") {
			deps.textContent = "results show here...";
			return;
		}
		deps.innerHTML = "";
		for (const root of [...prevRoots.keys()]
			.sort((a, b) => fuzzyCost(input.value, a) - fuzzyCost(input.value, b))) {
			const pane = document.createElement("div");
			const code = modifyCode(inst, localStorage["package-" + root], "fmt");
			pane.innerHTML = `<div>${root}</div><pre>${code}</pre>`;
			deps.appendChild(pane);
		}
		if (deps.innerHTML === "") {
			deps.textContent = "no results";
		}
	}

	Object.assign(window, { filterCodeDeps });
});

/** @param {HTMLElement} target  */
function runPost(target) {
	while (!target.matches("div[class=preview]")) target = target.parentElement ?? never();
	const code = target.querySelector("pre[apply=fmt]");
	if (!(code instanceof HTMLPreElement)) never();
	const output = target.querySelector("pre[id=compiler-output]");
	if (!(output instanceof HTMLPreElement)) never();

	Promise.all([getHbcInstance(), getFmtInstance()]).then(async ([hbc, fmt]) => {
		const ctx = { keyBuf: [], prevParams: new Set() };
		await fetchPackages(code.innerText ?? never(), new Set(), output, ctx);
		const posts = [{ path: "this", code: "" }];
		loadCachedPackages(fmt, code.innerText ?? never(), ctx.keyBuf, posts, new Set());
		output.textContent = compileCode(hbc, posts);
		output.hidden = false;
	});

	let author = encodeURIComponent(target.dataset.author ?? never());
	let name = encodeURIComponent(target.dataset.name ?? never());
	fetch(`/post/run?author=${author}&name=${name}`, { method: "POST" })
}

Object.assign(window, { runPost });

updateTab();
wireUp(document.body);

