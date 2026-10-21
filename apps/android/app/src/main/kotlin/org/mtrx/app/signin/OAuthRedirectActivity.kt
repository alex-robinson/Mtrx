package org.mtrx.app.signin

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import org.mtrx.app.MainActivity
import org.mtrx.app.MtrxApplication

/**
 * Receives the browser sign-in's redirect (oauthRedirectUri(),
 * io.github.alex-robinson:/oauth?…), hands it to the account and returns
 * to the app, closing the Custom Tab.
 */
class OAuthRedirectActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val uri = intent?.data
        if (uri != null && uri.path == "/oauth") {
            (application as MtrxApplication).account.finishBrowserLogin(uri.toString())
        }
        startActivity(
            Intent(this, MainActivity::class.java).addFlags(
                Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP or
                    Intent.FLAG_ACTIVITY_SINGLE_TOP,
            ),
        )
        finish()
    }
}
