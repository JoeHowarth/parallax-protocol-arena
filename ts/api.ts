import { FromJs, ToJs } from "../bindings/bindings.ts";

export function send(fromJs: FromJs) {
	return runjs.send(fromJs);
}

export function recv(): Promise<ToJs> {
	return runjs.recv();
}

// export function query(key: string): QueryResult {
// 	send({ query: key });
// }
