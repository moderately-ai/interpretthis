# Pins: ExceptionGroup + except* (PEP 654).
eg = ExceptionGroup("boom", [ValueError("a"), TypeError("b")])
print(type(eg).__name__)
print(len(eg.exceptions))

caught = []
try:
    raise eg
except* ValueError as e:
    caught.append(("VE", len(e.exceptions), type(e.exceptions[0]).__name__))
except* TypeError as e:
    caught.append(("TE", len(e.exceptions), type(e.exceptions[0]).__name__))
print(sorted(caught))
