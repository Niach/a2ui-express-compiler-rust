# Login form

Request: A sign-in screen: email and password fields (both required, the email validated as an email), a 'Remember me' checkbox, a primary 'Sign in' button, and a 'Forgot password?' link.

## Sample A

```
Page title="Sign in"
  Card
    Form name="signin"
      Input label="Email" value={"@path":"/email"} name="email" type="email" checks=[{"condition":{"@call":"required","args":{"value":{"@path":"/email"}}},"message":, {"condition":{"@call":"email","args":{"value":{"@path":"/email"}}},"message":"En]
      Input label="Password" value={"@path":"/password"} name="password" type="password" checks=[{"condition":{"@call":"required","args":{"value":{"@path":"/password"}}},"messag]
      Checkbox label="Remember me" name="remember"
      Button label="Sign in" variant="default"
      Link label="Forgot password?"
[data model] / = {"email":"","password":"","remember":false}
```

## Sample B

```
Page title="Sign in"
  Form name="signin"
    Input label="Email" value={"@path":"/form/email"} name="email" type="email" checks=[{"condition":{"@call":"required","args":{"value":{"@path":"/form/email"}}},"mess, {"condition":{"@call":"email","args":{"value":{"@path":"/form/email"}}},"message]
    Input label="Password" value={"@path":"/form/password"} name="password" type="password" checks=[{"condition":{"@call":"required","args":{"value":{"@path":"/form/password"}}},"m]
    Checkbox label="Remember me" name="remember"
    FormActions
  Link label="Forgot password?"
[data model] {"form":{"email":"","password":"","remember":false}}
```
