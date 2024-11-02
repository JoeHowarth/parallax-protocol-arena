// runtime.d.ts
// runtime.d.ts

// First, declare the interfaces
// interface DenoCore {
// 	print(msg: string, isErr: boolean): void;
// 	ops: {
// 		op_sleep(ms: number): Promise<void>;
// 		op_send(msg: string): Promise<void>;
// 		op_recv(): Promise<string>;
// 		op_read_file(path: string): Promise<string>;
// 		op_write_file(path: string, contents: string): Promise<void>;
// 		op_remove_file(path: string): Promise<void>;
// 		op_fetch(url: string): Promise<string>;
// 	};
// }
//
// interface Deno_ {
// 	core: DenoCore;
// }

// Then declare the global variable
declare global {
	interface Runjs {
		sleep(ms: number): Promise<void>;
		send(msg: string): Promise<void>;
		recv(): Promise<string>;
		readFile(path: string): Promise<string>;
		writeFile(path: string, contents: string): Promise<void>;
		removeFile(path: string): Promise<void>;
		fetch(url: string): Promise<string>;
	}

	const runjs: Runjs;
	// const Deno: Deno_;
}

export {};
