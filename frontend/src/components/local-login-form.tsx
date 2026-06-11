import { useForm } from "react-hook-form"
import { zodResolver } from "@hookform/resolvers/zod"
import { useQueryClient } from "@tanstack/react-query"
import { z } from "zod"

import { api, ApiError, queryKeys } from "@/lib/api"
import { Button } from "@/components/ui/button"
import { Field, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"

const schema = z.object({
  username: z.string().trim().min(1, "required"),
  password: z.string().min(1, "required"),
})
type FormValues = z.infer<typeof schema>

/** Local operator sign-in (no GitLab identity — administration only). */
export function LocalLoginForm() {
  const queryClient = useQueryClient()
  const form = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: { username: "", password: "" },
  })

  const onSubmit = form.handleSubmit(async (values) => {
    try {
      await api<void>("/auth/local", { method: "POST", body: JSON.stringify(values) })
      await queryClient.invalidateQueries({ queryKey: queryKeys.me })
    } catch (err) {
      form.setError("password", {
        message:
          err instanceof ApiError && err.status === 401
            ? "Invalid username or password."
            : "Sign-in failed — try again.",
      })
    }
  })

  return (
    <form onSubmit={onSubmit} noValidate>
      <FieldGroup className="gap-3">
        <Field data-invalid={!!form.formState.errors.username}>
          <FieldLabel htmlFor="local-username">Username</FieldLabel>
          <Input id="local-username" autoComplete="username" {...form.register("username")} />
          {form.formState.errors.username ? (
            <FieldError>{form.formState.errors.username.message}</FieldError>
          ) : null}
        </Field>
        <Field data-invalid={!!form.formState.errors.password}>
          <FieldLabel htmlFor="local-password">Password</FieldLabel>
          <Input
            id="local-password"
            type="password"
            autoComplete="current-password"
            {...form.register("password")}
          />
          {form.formState.errors.password ? (
            <FieldError>{form.formState.errors.password.message}</FieldError>
          ) : null}
        </Field>
        <Button
          type="submit"
          variant="secondary"
          className="w-full"
          disabled={form.formState.isSubmitting}
        >
          {form.formState.isSubmitting ? "Signing in…" : "Sign in"}
        </Button>
      </FieldGroup>
    </form>
  )
}
