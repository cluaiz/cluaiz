/* CEP (Cluaiz Engine Protocol) over the C ABI: the Cluaiz Engine inside your process.
 *
 * Library: cluaiz_cep.dll (Windows), libcluaiz_cep.so (Linux), libcluaiz_cep.dylib (macOS).
 * Messages are JSON-RPC 2.0, the same as POST /v1/cep. Discover the methods with
 *   {"jsonrpc":"2.0","id":1,"method":"rpc.discover"}
 *
 * The first call starts the Engine (home from CLUAIZ_HOME, or the default) and may take a while.
 * cep_call blocks until the answer and may be called from several threads, but not from a thread
 * that is running an async runtime. Long work returns a task: poll it with tasks.get / tasks.result.
 */
#ifndef CLUAIZ_CEP_H
#define CLUAIZ_CEP_H

#ifdef __cplusplus
extern "C" {
#endif

/* request_json: a UTF-8 JSON-RPC request or batch. Returns the UTF-8 JSON reply, or NULL when the
 * input held only notifications. Free the reply with cep_free. */
char *cep_call(const char *request_json);

/* Frees a reply of cep_call. NULL is ignored. Never use the C runtime's free on a reply. */
void cep_free(char *reply);

#ifdef __cplusplus
}
#endif

#endif /* CLUAIZ_CEP_H */
