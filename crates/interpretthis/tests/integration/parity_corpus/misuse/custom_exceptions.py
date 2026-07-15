class ValidationError(Exception):
    def __init__(self, field, message):
        self.field = field
        self.message = message
        super().__init__(f"{field}: {message}")
try:
    raise ValidationError("email", "invalid format")
except ValidationError as e:
    print(e.field, e.message)
    print(str(e))
class NotFoundError(Exception):
    pass
try:
    raise NotFoundError("resource missing")
except Exception as e:
    print(type(e).__name__, str(e))
class ChainedError(Exception):
    pass
try:
    try:
        raise ValueError("root cause")
    except ValueError as e:
        raise ChainedError("wrapper") from e
except ChainedError as e:
    print(str(e), "|", str(e.__cause__))
