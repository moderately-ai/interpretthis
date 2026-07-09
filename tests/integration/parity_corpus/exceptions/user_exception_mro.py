# User exception classes: except Parent catches Child; except ValueError
# catches classes that subclass ValueError.
class AppError(Exception):
    pass

class ValidationError(AppError):
    pass

try:
    raise ValidationError('bad')
except AppError as e:
    print(type(e).__name__)

try:
    raise ValidationError('bad2')
except Exception as e:
    print('caught')

try:
    raise ValidationError('x')
except ValueError:
    print('nope')
except AppError:
    print('app')
