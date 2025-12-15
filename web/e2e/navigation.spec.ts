import { test, expect } from '@playwright/test'

test.describe('Navigation', () => {
  test('should navigate to dashboard', async ({ page }) => {
    await page.goto('/')
    await expect(page).toHaveTitle(/Hodei/)
    await expect(page.locator('text=Dashboard')).toBeVisible()
  })

  test('should navigate to jobs page via bottom nav', async ({ page }) => {
    await page.goto('/')
    await page.click('text=Jobs')
    await expect(page).toHaveURL('/jobs')
    await expect(page.locator('text=Job History')).toBeVisible()
  })

  test('should navigate to metrics page via bottom nav', async ({ page }) => {
    await page.goto('/')
    await page.click('text=Metrics')
    await expect(page).toHaveURL('/metrics')
    await expect(page.locator('text=System Overview')).toBeVisible()
  })

  test('should navigate to providers page via bottom nav', async ({ page }) => {
    await page.goto('/')
    await page.click('text=Providers')
    await expect(page).toHaveURL('/providers')
    await expect(page.locator('h1:has-text("Providers")')).toBeVisible()
  })
})

test.describe('Dashboard Page', () => {
  test('should display stats cards', async ({ page }) => {
    await page.goto('/')
    await expect(page.locator('text=Total Jobs')).toBeVisible()
    await expect(page.locator('text=Running')).toBeVisible()
    await expect(page.locator('text=Failed')).toBeVisible()
    await expect(page.locator('text=Success')).toBeVisible()
  })

  test('should display recent executions', async ({ page }) => {
    await page.goto('/')
    await expect(page.locator('text=Recent Executions')).toBeVisible()
  })
})

test.describe('Job History Page', () => {
  test('should display filter chips', async ({ page }) => {
    await page.goto('/jobs')
    await expect(page.locator('button:has-text("All Jobs")')).toBeVisible()
    await expect(page.locator('button:has-text("Running")')).toBeVisible()
    await expect(page.locator('button:has-text("Failed")')).toBeVisible()
  })

  test('should have search input', async ({ page }) => {
    await page.goto('/jobs')
    await expect(page.locator('input[placeholder*="Search"]')).toBeVisible()
  })
})

test.describe('Metrics Page', () => {
  test('should display time range selector', async ({ page }) => {
    await page.goto('/metrics')
    await expect(page.locator('text=1H')).toBeVisible()
    await expect(page.locator('text=24H')).toBeVisible()
    await expect(page.locator('text=7D')).toBeVisible()
    await expect(page.locator('text=30D')).toBeVisible()
  })

  test('should display KPI cards', async ({ page }) => {
    await page.goto('/metrics')
    await expect(page.locator('text=Total Jobs')).toBeVisible()
    await expect(page.locator('text=Avg Duration')).toBeVisible()
    await expect(page.locator('text=CPU Load')).toBeVisible()
  })
})

test.describe('Providers Page', () => {
  test('should display provider list', async ({ page }) => {
    await page.goto('/providers')
    await expect(page.locator('text=Docker Production')).toBeVisible()
  })

  test('should have filter chips', async ({ page }) => {
    await page.goto('/providers')
    await expect(page.locator('button:has-text("All")')).toBeVisible()
    await expect(page.locator('button:has-text("Active")')).toBeVisible()
    await expect(page.locator('button:has-text("Unhealthy")')).toBeVisible()
  })
})
