# Pins: bare `raise` in an except handler re-raises the active
# exception unchanged. Idiomatic in cleanup-then-rethrow patterns.
def cleanup_and_propagate():
    try:
        raise ValueError('original')
    except ValueError:
        # Imagine cleanup work here.
        raise

try:
    cleanup_and_propagate()
except ValueError as e:
    print(type(e).__name__)
    print(str(e))
