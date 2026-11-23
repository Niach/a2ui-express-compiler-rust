# Sign-up form

Request: A create-account screen: full name, email (validated as an email), password and confirm password (both required, at least 8 characters), an 'I agree to the terms' checkbox that must be checked, and a 'Create account' button.

## Sample A

```
Page title="Create account"
  Form name="signup"
    Stack
      Input label="Full name" value={"@path":"/signup/name"} name="name" type="text"
      Input label="Email" value={"@path":"/signup/email"} name="email" type="email" checks=[{"condition":{"@call":"email","args":{"value":{"@path":"/signup/email"}}},"messa]
      Input label="Password" value={"@path":"/signup/password"} name="password" type="password" checks=[{"condition":{"@call":"required","args":{"value":{"@path":"/signup/password"}}},, {"condition":{"@call":"length","args":{"value":{"@path":"/signup/password"},"min]
      Input label="Confirm password" value={"@path":"/signup/confirm"} name="confirm" type="password" checks=[{"condition":{"@call":"required","args":{"value":{"@path":"/signup/confirm"}}},", {"condition":{"@call":"length","args":{"value":{"@path":"/signup/confirm"},"min"]
      Checkbox label="I agree to the terms" name="agree" checks=[{"condition":{"@call":"eq","args":{"a":{"@path":"/signup/agree"},"b":true}},"mes]
    FormActions
[data model] /signup = {"name":"","email":"","password":"","confirm":"","agree":false}
```

## Sample B

```
Page title="Create account"
  Form name="signup"
    Input label="Full name" value={"@path":"/form/name"} name="name"
    Input label="Email" value={"@path":"/form/email"} name="email" type="email" checks=[{"condition":{"@call":"required","args":{"value":{"@path":"/form/email"}}},"mess, {"condition":{"@call":"email","args":{"value":{"@path":"/form/email"}}},"message]
    Input label="Password" value={"@path":"/form/password"} name="password" type="password" checks=[{"condition":{"@call":"required","args":{"value":{"@path":"/form/password"}}},"m, {"condition":{"@call":"length","args":{"value":{"@path":"/form/password"},"min":]
    Input label="Confirm password" value={"@path":"/form/confirm"} name="confirm" type="password" checks=[{"condition":{"@call":"required","args":{"value":{"@path":"/form/confirm"}}},"me, {"condition":{"@call":"length","args":{"value":{"@path":"/form/confirm"},"min":8]
    Checkbox label="I agree to the terms" name="terms" checks=[{"condition":{"@call":"required","args":{"value":{"@path":"/form/terms"}}},"mess]
    FormActions
[data model] {"form":{"name":"","email":"","password":"","confirm":"","terms":false}}
```
