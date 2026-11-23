# Sign-up form

Request: A create-account screen: full name, email (validated as an email), password and confirm password (both required, at least 8 characters), an 'I agree to the terms' checkbox that must be checked, and a 'Create account' button.

## Sample A

```
Page title="Create account"
  Card title="Sign up"
    Form name="signup" on=[submit→event create_account {name: $/form/name, email: $/form/email, password: $/form/password}]
      Input label="Full name" name="name" value=$/form/name placeholder="Jane Doe" checks=[required(value=$/form/name) "Name is required"]
      Input label="Email" name="email" type="email" value=$/form/email placeholder="you@example.com" checks=[required(value=$/form/email) "Email is required", email(value=$/form/email) "Enter a valid email address"]
      Input label="Password" name="password" type="password" value=$/form/password checks=[required(value=$/form/password) "Password is required", length(value=$/form/password, min=8) "At least 8 characters"]
      Input label="Confirm password" name="confirm" type="password" value=$/form/confirm checks=[required(value=$/form/confirm) "Please confirm your password", length(value=$/form/confirm, min=8) "At least 8 characters"]
      Checkbox label="I agree to the terms" name="terms" checked=$/form/terms checks=[required(value=$/form/terms) "You must agree to the terms"]
      FormActions submit="Create account"
[data model] {"form":{"name":"","email":"","password":"","confirm":"","terms":false}}
```

## Sample B

```
Page title="Create account"
  Form name="signup" on=[submit→event signup {fullName: $/fullName, email: $/email, password: $/password}]
    Stack gap="md"
      Input label="Full name" name="fullName" type="text" value=$/fullName checks=[required(value=$/fullName) "Enter your full name"]
      Input label="Email" name="email" type="email" value=$/email checks=[email(value=$/email) "Enter a valid email address"]
      Input label="Password" name="password" type="password" value=$/password checks=[required(value=$/password) "Password is required", length(value=$/password, min=8) "Use at least 8 characters"]
      Input label="Confirm password" name="confirmPassword" type="password" value=$/confirmPassword checks=[required(value=$/confirmPassword) "Confirm your password", length(value=$/confirmPassword, min=8) "Use at least 8 characters"]
    Checkbox label="I agree to the terms" name="agree" checked=$/agree checks=[eq(a=$/agree, b=true) "You must agree to the terms"]
    FormActions submit="Create account"
[data model] / = {"fullName":"","email":"","password":"","confirmPassword":"","agree":false}
```
