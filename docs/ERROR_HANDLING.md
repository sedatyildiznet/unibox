# Error handling

Unibox should surface errors in user terms while preserving technical detail for diagnostics. Runtime and connector operations return structured failure states rather than silently failing. Recoverable operations should retry with bounded backoff; repeated crashes must stop auto-restart loops and expose a clear recovery action. Update failures should preserve the previous known-good connector/application state whenever possible.
