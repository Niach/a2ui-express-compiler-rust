# Login form

Request: A sign-in screen: email and password fields (both required, the email validated as an email), a 'Remember me' checkbox, a primary 'Sign in' button, and a 'Forgot password?' link.

## Sample A

```
Page title="Sign in"
  Card title="Sign in"
    Form name="signin" on=[submit→event sign_in {email: $/form/email, password: $/form/password, remember: $/form/remember}]
      Input label="Email" name="email" type="email" placeholder="you@example.com" value=$/form/email checks=[required(value=$/form/email) "Email is required", email(value=$/form/email) "Enter a valid email address"]
      Input label="Password" name="password" type="password" value=$/form/password checks=[required(value=$/form/password) "Password is required"]
      Checkbox label="Remember me" name="remember" checked=$/form/remember
      Button label="Sign in" submit=true
      Link label="Forgot password?" on=[press→event forgot_password]
[data model] {"form":{"email":"","password":"","remember":false}}
```

## Sample B

```
Page title="Sign in"
  Card title="Welcome back" description="Sign in to your account to continue."
    Form name="signin" on=[submit→event signin {email: $/email, password: $/password, remember: $/remember}]
      Stack gap="md"
        Input name="email" label="Email" type="email" placeholder="you@example.com" value=$/email validateOn="blur" checks=[required(value=$/email) "Email is required.", email(value=$/email) "Enter a valid email address."]
        Input name="password" label="Password" type="password" value=$/password checks=[required(value=$/password) "Password is required."]
        Checkbox name="remember" label="Remember me" checked=$/remember
        Button label="Sign in" variant="default" submit=true
        Link label="Forgot password?" on=[press→event forgotPassword]
[data model] / = {"email":"","password":"","remember":false}
```
