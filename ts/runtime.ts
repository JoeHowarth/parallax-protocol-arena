//@ts-ignore asdf
const { core } = Deno;

import { FromJs, ToJs } from "../bindings/bindings.ts";

//@ts-ignore asdf
function argsToMessage(...args) {
	return args.map((arg) => JSON.stringify(arg)).join(" ");
}

//@ts-ignore asdf
globalThis.console = {
	log: (...args) => {
		core.print(`[out]: ${argsToMessage(...args)}\n`, false);
	},
	error: (...args) => {
		core.print(`[err]: ${argsToMessage(...args)}\n`, true);
	},
};

//@ts-ignore asdf
globalThis.runjs = {
	sleep: (ms: number) => {
		return core.ops.op_sleep(ms);
	},
	send: (msg: FromJs) => {
		return core.ops.op_send(msg);
	},
	recv: (): ToJs => {
		return core.ops.op_recv();
	},
	readFile: (path: string) => {
		return core.ops.op_read_file(path);
	},
	writeFile: (path: string, contents: string) => {
		return core.ops.op_write_file(path, contents);
	},
	removeFile: (path: string) => {
		return core.ops.op_remove_file(path);
	},
	fetch: (url: string) => {
		return core.ops.op_fetch(url);
	},
};
