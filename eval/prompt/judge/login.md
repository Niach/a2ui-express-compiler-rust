# Login form

Request: A sign-in screen: email and password fields (both required, the email validated as an email), a 'Remember me' checkbox, a primary 'Sign in' button, and a 'Forgot password?' link.

## Sample A

```
Page title="Sign in"
  Form name="signin"
    Input label="Email" placeholder="you@example.com" name="email" type="email" checks=[{"condition":{"@call":"required","args":{"value":"Email is required"}},"message", {"condition":{"@call":"email","args":{"value":"Enter a valid email address"}},"m]
    Input label="Password" name="password" type="password" checks=[{"condition":{"@call":"required","args":{"value":"Password is required"}},"messa]
    Checkbox label="Remember me" name="remember"
    Link label="Forgot password?"
    Button label="Sign in" variant="default"
```

## Sample B

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
