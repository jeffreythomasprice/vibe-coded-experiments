Prefer code that is easily testable without mocking or other workarounds. Write tests for that code.

Handle errors at the lowest sensible location. Always do something with errors, even if it's just adding context with `fmt.Errorf` and a `%w` argument and returning the new error.